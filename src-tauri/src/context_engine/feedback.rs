use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
            "confirm_context" | "confirm" | "confirm_membership" | "membership_confirmed" | "context_confirmed" => Ok(Self::ConfirmContext),
            "reject_context" | "reject" | "reject_membership" | "membership_dismissed" | "context_dismissed" => Ok(Self::RejectContext),
            "remove_from_context" | "remove" | "unassign" | "user_removed_from_context" => Ok(Self::RemoveFromContext),
            "mark_important" | "pin_memory" | "set_important" | "keep_important" => Ok(Self::MarkImportant),
            "remove_importance" | "unpin_memory" | "unset_important" | "remove_from_important" => Ok(Self::RemoveImportance),
            "confirm_task" | "accept_task" | "task_confirmed" => Ok(Self::ConfirmTask),
            "reject_task" | "dismiss_task" | "task_dismissed" | "task_rejected" => Ok(Self::RejectTask),
            "dismiss_suggestion" | "dismiss" | "dismiss_inbox" | "suggestion_dismissed" => Ok(Self::DismissSuggestion),
            "resume_context" | "resume" | "context_resumed" => Ok(Self::ResumeContext),
            "archive_context" | "archive" | "context_archived" => Ok(Self::ArchiveContext),
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

    pub fn infer_from_id(id: impl Into<String>) -> Self {
        let s = id.into();
        if s.starts_with("task_") || s.starts_with("task-") {
            Self::Task(s)
        } else if s.starts_with("ws_") || s.starts_with("ctx_") || s.starts_with("context_") {
            Self::Context(s)
        } else if s.starts_with("session_") || s.starts_with("sess_") {
            Self::Session(s)
        } else if s.starts_with("mem_") || s.starts_with("memory_") {
            Self::Memory(s)
        } else if s.starts_with("sug_") || s.starts_with("suggestion_") {
            Self::Suggestion(s)
        } else if s.starts_with("cand_") || s.starts_with("candidate_") {
            Self::Candidate(s)
        } else {
            Self::Item(s)
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
    /// Automated background heuristic or implicit feedback.
    ImplicitInference,
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
            Self::ImplicitInference => "implicit_inference",
            Self::DiagnosticHarness => "diagnostic_harness",
        }
    }

    /// Whether this feedback source corresponds to a direct explicit user action.
    pub fn is_explicit_user(&self) -> bool {
        match self {
            Self::UserExplicitUi
            | Self::ShortcutKey
            | Self::InboxAction
            | Self::WorksetTray
            | Self::ContextMenu
            | Self::MemoryInspectionPanel
            | Self::SystemNotification => true,
            Self::ImplicitInference | Self::DiagnosticHarness => false,
        }
    }
}

impl Default for FeedbackSource {
    fn default() -> Self {
        Self::UserExplicitUi
    }
}

/// Explicit scope of the feedback's effect.
///
/// Invariant: Always default to the narrowest reasonable scope.
/// A user removing an item or rejecting a repository from a workspace
/// must never produce unintended global or broad cross-project side effects.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "scope_type", rename_all = "snake_case")]
pub enum FeedbackScope {
    /// 1. Narrowest: Scoped specifically to a single item within a specific context.
    /// (e.g., removing a specific web article from PayX).
    ItemContext {
        item_id: String,
        context_id: String,
    },
    /// 2. Scoped to a specific telemetry source (app/domain/tool) within a specific context.
    /// (e.g., dismissing generic media alerts specifically from PayX).
    SourceContext {
        source_id: String,
        context_id: String,
    },
    /// 3. Scoped to a specific task candidate or active task within a specific context.
    /// (e.g., confirming task 'Optimize Checkout Flow' in PayX).
    TaskContext {
        task_id: String,
        context_id: String,
    },
    /// 4. Scoped to the entire context itself.
    /// (e.g., resuming, archiving, or pausing Workspace PayX).
    Context {
        context_id: String,
    },
    /// 5. Scoped to an explicit entity-to-entity relationship.
    /// (e.g., binding Git repository 'github.com/org/repo-x' specifically to PayX).
    Relationship {
        source_ref: String,
        target_ref: String,
        relationship_kind: String,
    },
    /// 6. Broadest: System-wide global feedback (e.g. system noise / password manager filter).
    /// Invariant: NEVER automatically inferred; requires explicit user action.
    Global,
}

impl FeedbackScope {
    // -------------------------------------------------------------------------
    // Constructors
    // -------------------------------------------------------------------------

    pub fn item_context(item_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self::ItemContext {
            item_id: item_id.into(),
            context_id: context_id.into(),
        }
    }

    pub fn source_context(source_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self::SourceContext {
            source_id: source_id.into(),
            context_id: context_id.into(),
        }
    }

    pub fn task_context(task_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self::TaskContext {
            task_id: task_id.into(),
            context_id: context_id.into(),
        }
    }

    pub fn context(context_id: impl Into<String>) -> Self {
        Self::Context {
            context_id: context_id.into(),
        }
    }

    pub fn relationship(
        source_ref: impl Into<String>,
        target_ref: impl Into<String>,
        relationship_kind: impl Into<String>,
    ) -> Self {
        Self::Relationship {
            source_ref: source_ref.into(),
            target_ref: target_ref.into(),
            relationship_kind: relationship_kind.into(),
        }
    }

    pub fn global() -> Self {
        Self::Global
    }

    // -------------------------------------------------------------------------
    // Narrowest Scope Inference
    // -------------------------------------------------------------------------

    /// Automatically infers the narrowest reasonable scope for an action and target entity.
    /// Guarantees that item removals default to ItemContext and task actions to TaskContext.
    pub fn infer_narrowest_scope(
        action: FeedbackAction,
        target_ref: &FeedbackTargetRef,
        context_ref: Option<&str>,
    ) -> Self {
        match (action, target_ref, context_ref) {
            // Context lifecycle actions -> Context scope
            (FeedbackAction::ResumeContext | FeedbackAction::ArchiveContext, _, Some(ws_id)) => {
                Self::context(ws_id)
            }
            (FeedbackAction::ResumeContext | FeedbackAction::ArchiveContext, FeedbackTargetRef::Context(ws_id), None) => {
                Self::context(ws_id)
            }

            // Task feedback -> TaskContext scope
            (FeedbackAction::ConfirmTask | FeedbackAction::RejectTask, FeedbackTargetRef::Task(task_id), Some(ws_id)) => {
                Self::task_context(task_id, ws_id)
            }

            // Item feedback -> ItemContext scope (narrowest!)
            (
                FeedbackAction::ConfirmContext
                | FeedbackAction::RejectContext
                | FeedbackAction::RemoveFromContext
                | FeedbackAction::MarkImportant
                | FeedbackAction::RemoveImportance,
                FeedbackTargetRef::Item(item_id),
                Some(ws_id),
            ) => Self::item_context(item_id, ws_id),

            // Memory feedback with context
            (FeedbackAction::MarkImportant | FeedbackAction::RemoveImportance, FeedbackTargetRef::Memory(mem_id), Some(ws_id)) => {
                Self::item_context(mem_id, ws_id)
            }

            // Suggestion dismissal with context
            (FeedbackAction::DismissSuggestion, FeedbackTargetRef::Suggestion(sugg_id), Some(ws_id)) => {
                Self::item_context(sugg_id, ws_id)
            }

            // Fallback for actions with context
            (_, _, Some(ws_id)) => Self::context(ws_id),

            // If completely unassociated with any context, falls back to Global
            _ => Self::Global,
        }
    }

    // -------------------------------------------------------------------------
    // Scope Introspection & Matching
    // -------------------------------------------------------------------------

    /// Scope type identifier string.
    pub fn scope_str(&self) -> &'static str {
        match self {
            Self::ItemContext { .. } => "item_context",
            Self::SourceContext { .. } => "source_context",
            Self::TaskContext { .. } => "task_context",
            Self::Context { .. } => "context",
            Self::Relationship { .. } => "relationship",
            Self::Global => "global",
        }
    }

    /// Scope granularity rank: 1 (narrowest / most specific) to 6 (broadest).
    pub fn granularity_rank(&self) -> u8 {
        match self {
            Self::ItemContext { .. } => 1,
            Self::Relationship { .. } => 2,
            Self::TaskContext { .. } => 3,
            Self::SourceContext { .. } => 4,
            Self::Context { .. } => 5,
            Self::Global => 6,
        }
    }

    /// Scope specificity order (1 is most specific, 6 is broadest).
    pub fn specificity_order(&self) -> u8 {
        self.granularity_rank()
    }

    /// Associated workspace / context ID if the scope is bound to a context.
    pub fn context_id(&self) -> Option<&str> {
        match self {
            Self::ItemContext { context_id, .. } => Some(context_id.as_str()),
            Self::SourceContext { context_id, .. } => Some(context_id.as_str()),
            Self::TaskContext { context_id, .. } => Some(context_id.as_str()),
            Self::Context { context_id } => Some(context_id.as_str()),
            Self::Relationship { target_ref, .. } => {
                // If target or source is a context ref (e.g. ws:PayX or context:PayX)
                if let Some(ws) = target_ref.strip_prefix("context:").or_else(|| target_ref.strip_prefix("ws:")) {
                    Some(ws)
                } else {
                    None
                }
            }
            Self::Global => None,
        }
    }

    /// Backward compatibility helper: checks if the scope is workspace-scoped.
    pub fn is_workspace_scoped(&self) -> bool {
        self.context_id().is_some()
    }

    /// Backward compatibility helper: gets workspace_id.
    pub fn workspace_id(&self) -> Option<&str> {
        self.context_id()
    }

    /// Checks if this scope is strictly isolated from another workspace.
    /// Returns true if this scope CANNOT affect the specified target workspace.
    pub fn is_isolated_from(&self, other_context_id: &str) -> bool {
        match self {
            Self::ItemContext { context_id, .. }
            | Self::SourceContext { context_id, .. }
            | Self::TaskContext { context_id, .. }
            | Self::Context { context_id } => context_id != other_context_id,
            Self::Relationship { target_ref, source_ref, .. } => {
                let affects_target = target_ref.contains(other_context_id) || source_ref.contains(other_context_id);
                !affects_target
            }
            Self::Global => false, // Global is NOT isolated from other workspaces
        }
    }

    /// Evaluates whether this feedback scope applies to a candidate incoming event.
    pub fn applies_to(
        &self,
        candidate_item_id: Option<&str>,
        candidate_source_id: Option<&str>,
        candidate_task_id: Option<&str>,
        candidate_context_id: &str,
    ) -> bool {
        match self {
            Self::ItemContext { item_id, context_id } => {
                context_id == candidate_context_id && candidate_item_id == Some(item_id.as_str())
            }
            Self::SourceContext { source_id, context_id } => {
                context_id == candidate_context_id && candidate_source_id == Some(source_id.as_str())
            }
            Self::TaskContext { task_id, context_id } => {
                context_id == candidate_context_id && candidate_task_id == Some(task_id.as_str())
            }
            Self::Context { context_id } => context_id == candidate_context_id,
            Self::Relationship { source_ref, target_ref, .. } => {
                let context_matches = target_ref.contains(candidate_context_id) || source_ref.contains(candidate_context_id);
                let item_matches = candidate_item_id.map(|id| source_ref.contains(id) || target_ref.contains(id)).unwrap_or(false);
                let source_matches = candidate_source_id.map(|src| source_ref.contains(src) || target_ref.contains(src)).unwrap_or(false);
                context_matches && (item_matches || source_matches)
            }
            Self::Global => true,
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

    /// Sets an optional user or system reason.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
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
    // Factory Helpers for Supported Feedback Types (Defaulting to Narrowest Scope)
    // -------------------------------------------------------------------------

    /// Factory: User confirms an item into a context workspace (Narrowest: ItemContext).
    pub fn confirm_context(
        item_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
    ) -> Self {
        let i_id = item_id.into();
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::ConfirmContext,
            FeedbackTargetRef::Item(i_id.clone()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::item_context(i_id, ws_id),
            Some("User confirmed item context association".to_string()),
        )
    }

    /// Factory: User rejects a suggested context association for an item (Narrowest: ItemContext).
    pub fn reject_context(
        item_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let i_id = item_id.into();
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::RejectContext,
            FeedbackTargetRef::Item(i_id.clone()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::item_context(i_id, ws_id),
            reason.or(Some("User rejected context suggestion".to_string())),
        )
    }

    /// Factory: User explicitly removes an existing item from a context workspace (Narrowest: ItemContext).
    pub fn remove_from_context(
        item_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let i_id = item_id.into();
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::RemoveFromContext,
            FeedbackTargetRef::Item(i_id.clone()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::item_context(i_id, ws_id),
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
            Some(ws) => FeedbackScope::item_context(target_ref.id(), ws),
            None => FeedbackScope::global(),
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
            Some(ws) => FeedbackScope::item_context(target_ref.id(), ws),
            None => FeedbackScope::global(),
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

    /// Factory: User confirms a task candidate in a context (Narrowest: TaskContext).
    pub fn confirm_task(
        task_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
    ) -> Self {
        let t_id = task_id.into();
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::ConfirmTask,
            FeedbackTargetRef::Task(t_id.clone()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::task_context(t_id, ws_id),
            Some("User confirmed task in context".to_string()),
        )
    }

    /// Factory: User rejects a task candidate (Narrowest: TaskContext).
    pub fn reject_task(
        task_id: impl Into<String>,
        workspace_id: Option<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let t_id = task_id.into();
        let scope = match workspace_id.as_ref() {
            Some(ws) => FeedbackScope::task_context(t_id.clone(), ws),
            None => FeedbackScope::global(),
        };
        Self::new(
            FeedbackAction::RejectTask,
            FeedbackTargetRef::Task(t_id),
            workspace_id,
            source,
            scope,
            reason.or(Some("User rejected task candidate".to_string())),
        )
    }

    /// Factory: User dismisses a suggestion or inbox recommendation (Narrowest: ItemContext / TaskContext).
    pub fn dismiss_suggestion(
        suggestion_id: impl Into<String>,
        workspace_id: Option<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let s_id = suggestion_id.into();
        let scope = match workspace_id.as_ref() {
            Some(ws) => FeedbackScope::item_context(s_id.clone(), ws),
            None => FeedbackScope::global(),
        };
        Self::new(
            FeedbackAction::DismissSuggestion,
            FeedbackTargetRef::Suggestion(s_id),
            workspace_id,
            source,
            scope,
            reason.or(Some("User dismissed suggestion".to_string())),
        )
    }

    /// Factory: User explicitly resumes a context (Narrowest: Context scope).
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
            FeedbackScope::context(ws_id),
            Some("User explicitly resumed context".to_string()),
        )
    }

    /// Factory: User explicitly archives a context (Narrowest: Context scope).
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
            FeedbackScope::context(ws_id),
            reason.or(Some("User archived context".to_string())),
        )
    }

    /// Factory: User confirms or modifies an explicit entity-context relationship.
    pub fn relationship_feedback(
        action: FeedbackAction,
        source_entity: impl Into<String>,
        target_context: impl Into<String>,
        relationship_kind: impl Into<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let src = source_entity.into();
        let tgt = target_context.into();
        let kind = relationship_kind.into();
        Self::new(
            action,
            FeedbackTargetRef::Other {
                entity_type: kind.clone(),
                entity_id: src.clone(),
            },
            Some(tgt.clone()),
            source,
            FeedbackScope::relationship(src, tgt, kind),
            reason,
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
        match self.scope {
            FeedbackScope::ItemContext { ref item_id, ref context_id } => {
                if item_id.trim().is_empty() {
                    return Err("ItemContext feedback requires non-empty item_id".to_string());
                }
                if context_id.trim().is_empty() {
                    return Err("ItemContext feedback requires non-empty context_id".to_string());
                }
            }
            FeedbackScope::SourceContext { ref source_id, ref context_id } => {
                if source_id.trim().is_empty() {
                    return Err("SourceContext feedback requires non-empty source_id".to_string());
                }
                if context_id.trim().is_empty() {
                    return Err("SourceContext feedback requires non-empty context_id".to_string());
                }
            }
            FeedbackScope::TaskContext { ref task_id, ref context_id } => {
                if task_id.trim().is_empty() {
                    return Err("TaskContext feedback requires non-empty task_id".to_string());
                }
                if context_id.trim().is_empty() {
                    return Err("TaskContext feedback requires non-empty context_id".to_string());
                }
            }
            FeedbackScope::Context { ref context_id } => {
                if context_id.trim().is_empty() {
                    return Err("Context feedback requires non-empty context_id".to_string());
                }
            }
            FeedbackScope::Relationship { ref source_ref, ref target_ref, ref relationship_kind } => {
                if source_ref.trim().is_empty() || target_ref.trim().is_empty() || relationship_kind.trim().is_empty() {
                    return Err("Relationship feedback requires non-empty source_ref, target_ref, and relationship_kind".to_string());
                }
            }
            FeedbackScope::Global => {}
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
// Feedback Application Result
// =========================================================================

/// Structured record of the contextual systems affected by applying a feedback event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackApplicationResult {
    /// Originating feedback event ID
    pub event_id: String,
    /// Feedback action applied
    pub action: FeedbackAction,
    /// Scope enforced during application
    pub scope: FeedbackScope,
    /// Contextual subsystems mutated or notified
    pub applied_systems: Vec<String>,
    /// Evidence signal ID created in `context_signals`
    pub signal_id: Option<String>,
    /// Human-readable explanation of the applied changes
    pub details: String,
    /// Whether the feedback application succeeded without errors
    pub success: bool,
}

// =========================================================================
// Context Feedback Service
// =========================================================================

/// Central engine responsible for deterministically applying user feedback across
/// memberships, memory, tasks, inbox suggestions, and context lifecycle state.
pub struct ContextFeedbackService;

impl ContextFeedbackService {
    pub fn new() -> Self {
        Self
    }

    /// Applies a structured ContextFeedbackEvent to the appropriate contextual subsystems.
    ///
    /// Invariants:
    /// 1. **Zero Data Loss**: Raw items in `items` table are NEVER deleted.
    /// 2. **Scope Isolation**: Modifies only systems within the declared `FeedbackScope`.
    /// 3. **Auditable Evidence**: Persists a structured signal in `context_signals` for downstream consumers.
    /// 4. **Deterministic Invalidation**: Purges relevant in-memory caches to prevent stale reads.
    pub fn apply_feedback(
        conn: &rusqlite::Connection,
        event: &ContextFeedbackEvent,
    ) -> Result<FeedbackApplicationResult, String> {
        event.validate()?;

        let now = Utc::now().to_rfc3339();
        let mut applied_systems = Vec::new();
        let signal_id = Uuid::new_v4().to_string();
        let target_id = event.target_ref.id();

        match event.action {
            // -----------------------------------------------------------------
            // 1. Confirm Context Association
            // -----------------------------------------------------------------
            FeedbackAction::ConfirmContext => {
                let ws_id = event
                    .context_ref
                    .as_deref()
                    .or_else(|| event.scope.context_id())
                    .ok_or_else(|| "ConfirmContext requires a valid context reference".to_string())?;
                let event_ts = &event.timestamp;

                // A. Update context_memberships to confirmed (conf: 1.0, source: 'user')
                crate::db::operations::create_or_update_membership(
                    conn, target_id, ws_id, "confirmed", 1.0, "user",
                )
                .map_err(|e| format!("Failed to update membership: {}", e))?;
                applied_systems.push("context_memberships".to_string());

                // B. Set physical workspace_id on items
                conn.execute(
                    "UPDATE items SET workspace_id = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![ws_id, event_ts, target_id],
                )
                .map_err(|e| format!("Failed to set item workspace_id: {}", e))?;
                applied_systems.push("items".to_string());

                // C. Persist context signal evidence
                let payload = serde_json::json!({
                    "action": "confirm_context",
                    "item_id": target_id,
                    "workspace_id": ws_id,
                    "scope": event.scope.scope_str(),
                    "source": event.source.as_str(),
                    "reason": event.reason.as_deref().unwrap_or("User confirmed context association"),
                    "recorded_at": event_ts
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'user_feedback', NULL, ?4, ?5, ?5)",
                    rusqlite::params![signal_id, ws_id, target_id, payload, event_ts],
                )
                .map_err(|e| format!("Failed to insert context signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                // D. Invalidate caches
                Self::invalidate_caches_for_workspace(Some(ws_id));
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Confirmed item '{}' association with workspace '{}'", target_id, ws_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 2. Reject Context / Remove from Context
            // -----------------------------------------------------------------
            FeedbackAction::RejectContext | FeedbackAction::RemoveFromContext => {
                let ws_id = event
                    .context_ref
                    .as_deref()
                    .or_else(|| event.scope.context_id())
                    .ok_or_else(|| "RejectContext / RemoveFromContext requires a valid context reference".to_string())?;

                // Delegate to ContextCorrectionService for signature extraction and demotion with event timestamp
                let correction = crate::context_engine::correction::ContextCorrectionService::record_context_correction_with_time(
                    conn,
                    target_id,
                    ws_id,
                    event.reason.as_deref(),
                    Some(&event.timestamp),
                )?;
                applied_systems.push("context_correction_service".to_string());
                applied_systems.push("context_memberships".to_string());
                applied_systems.push("context_signals".to_string());
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(correction.id),
                    details: format!("Removed item '{}' from workspace '{}' and recorded negative signature", target_id, ws_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 3. Mark Important Memory
            // -----------------------------------------------------------------
            FeedbackAction::MarkImportant => {
                let ws_id = event
                    .context_ref
                    .as_deref()
                    .or_else(|| event.scope.context_id())
                    .ok_or_else(|| "MarkImportant requires a valid context reference".to_string())?;

                crate::context_engine::important_memory::record_context_memory_control(
                    conn,
                    ws_id,
                    target_id,
                    crate::context_engine::important_memory::UserMemoryControlAction::KeepImportant,
                )?;
                applied_systems.push("important_memory_service".to_string());
                applied_systems.push("context_memberships".to_string());

                let payload = serde_json::json!({
                    "action": "mark_important",
                    "item_id": target_id,
                    "workspace_id": ws_id,
                    "scope": event.scope.scope_str(),
                    "source": event.source.as_str(),
                    "reason": event.reason.as_deref().unwrap_or("User marked as important"),
                    "recorded_at": event.timestamp
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'user_feedback', NULL, ?4, ?5, ?5)",
                    rusqlite::params![signal_id, ws_id, target_id, payload, event.timestamp],
                )
                .map_err(|e| format!("Failed to insert memory signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                Self::invalidate_caches_for_workspace(Some(ws_id));
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Marked item/memory '{}' as important in workspace '{}'", target_id, ws_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 4. Remove Importance
            // -----------------------------------------------------------------
            FeedbackAction::RemoveImportance => {
                let ws_id = event
                    .context_ref
                    .as_deref()
                    .or_else(|| event.scope.context_id())
                    .ok_or_else(|| "RemoveImportance requires a valid context reference".to_string())?;

                crate::context_engine::important_memory::record_context_memory_control(
                    conn,
                    ws_id,
                    target_id,
                    crate::context_engine::important_memory::UserMemoryControlAction::RemoveFromImportant,
                )?;
                applied_systems.push("important_memory_service".to_string());
                applied_systems.push("context_memberships".to_string());

                let payload = serde_json::json!({
                    "action": "remove_importance",
                    "item_id": target_id,
                    "workspace_id": ws_id,
                    "scope": event.scope.scope_str(),
                    "source": event.source.as_str(),
                    "reason": event.reason.as_deref().unwrap_or("User removed importance status"),
                    "recorded_at": event.timestamp
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'user_feedback', NULL, ?4, ?5, ?5)",
                    rusqlite::params![signal_id, ws_id, target_id, payload, event.timestamp],
                )
                .map_err(|e| format!("Failed to insert memory signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                Self::invalidate_caches_for_workspace(Some(ws_id));
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Removed important memory status for '{}' in workspace '{}'", target_id, ws_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 5. Confirm Task Candidate
            // -----------------------------------------------------------------
            FeedbackAction::ConfirmTask => {
                let ws_id = event.context_ref.as_deref().or_else(|| event.scope.context_id());

                // Update task candidate status to Confirmed with 1.0 confidence
                crate::db::operations::confirm_task_candidate(
                    conn,
                    target_id,
                    1.0,
                )
                .map_err(|e| format!("Failed to update task candidate status: {}", e))?;
                applied_systems.push("task_candidates".to_string());

                // Persist signal
                let payload = serde_json::json!({
                    "action": "confirm_task",
                    "task_id": target_id,
                    "workspace_id": ws_id,
                    "scope": event.scope.scope_str(),
                    "reason": event.reason.as_deref().unwrap_or("User confirmed task in context"),
                    "recorded_at": event.timestamp
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, NULL, 'task_confirmed', NULL, ?3, ?4, ?4)",
                    rusqlite::params![signal_id, ws_id, payload, event.timestamp],
                )
                .map_err(|e| format!("Failed to insert task signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                Self::invalidate_caches_for_workspace(ws_id);
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Confirmed task candidate '{}' as user-confirmed active task", target_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 6. Reject Task Candidate
            // -----------------------------------------------------------------
            FeedbackAction::RejectTask => {
                let ws_id = event.context_ref.as_deref().or_else(|| event.scope.context_id());

                crate::db::operations::update_task_candidate_status(
                    conn,
                    target_id,
                    crate::db::models::TaskCandidateStatus::Dismissed,
                )
                .map_err(|e| format!("Failed to dismiss task candidate: {}", e))?;
                applied_systems.push("task_candidates".to_string());

                let payload = serde_json::json!({
                    "action": "reject_task",
                    "task_id": target_id,
                    "workspace_id": ws_id,
                    "scope": event.scope.scope_str(),
                    "reason": event.reason.as_deref().unwrap_or("User dismissed task candidate"),
                    "recorded_at": event.timestamp
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, NULL, 'task_dismissed', NULL, ?3, ?4, ?4)",
                    rusqlite::params![signal_id, ws_id, payload, event.timestamp],
                )
                .map_err(|e| format!("Failed to insert task dismissal signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                Self::invalidate_caches_for_workspace(ws_id);
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Dismissed task candidate '{}'", target_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 7. Dismiss Suggestion / Inbox Recommendation
            // -----------------------------------------------------------------
            FeedbackAction::DismissSuggestion => {
                let ws_id = event.context_ref.as_deref().or_else(|| event.scope.context_id());

                // Update inbox state to Dismissed
                let _ = crate::context_engine::inbox_state::InboxStateMachine::dismiss_item(
                    conn,
                    target_id,
                    event.reason.as_deref(),
                );
                applied_systems.push("inbox_state_machine".to_string());

                // If workspace specified, demote context_memberships
                if let Some(w) = ws_id {
                    let _ = crate::db::operations::create_or_update_membership(
                        conn, target_id, w, "dismissed", 0.0, "user",
                    );
                    applied_systems.push("context_memberships".to_string());
                }

                let payload = serde_json::json!({
                    "action": "dismiss_suggestion",
                    "item_id": target_id,
                    "workspace_id": ws_id,
                    "reason": event.reason.as_deref().unwrap_or("User dismissed suggestion"),
                    "recorded_at": now
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'suggestion_dismissed', NULL, ?4, ?5, ?5)",
                    rusqlite::params![signal_id, ws_id, target_id, payload, now],
                )
                .map_err(|e| format!("Failed to insert suggestion dismissal signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                Self::invalidate_caches_for_workspace(ws_id);
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Dismissed recommendation/suggestion '{}'", target_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 8. Resume Context
            // -----------------------------------------------------------------
            FeedbackAction::ResumeContext => {
                let ws_id = target_id;

                // Update workspace activity timestamp and lifecycle_state to active
                crate::db::operations::touch_workspace_activity(conn, ws_id)
                    .map_err(|e| format!("Failed to touch workspace: {}", e))?;
                conn.execute(
                    "UPDATE workspaces SET lifecycle_state = 'active', archived_at = NULL, last_activity_at = ?1, updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, ws_id],
                )
                .map_err(|e| format!("Failed to set lifecycle_state active: {}", e))?;
                applied_systems.push("workspaces".to_string());

                let payload = serde_json::json!({
                    "action": "resume_context",
                    "workspace_id": ws_id,
                    "recorded_at": now
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, NULL, 'context_resumed', NULL, ?3, ?4, ?4)",
                    rusqlite::params![signal_id, ws_id, payload, now],
                )
                .map_err(|e| format!("Failed to insert resume signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                Self::invalidate_caches_for_workspace(Some(ws_id));
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Resumed context workspace '{}' to active lifecycle state", ws_id),
                    success: true,
                })
            }

            // -----------------------------------------------------------------
            // 9. Archive Context
            // -----------------------------------------------------------------
            FeedbackAction::ArchiveContext => {
                let ws_id = target_id;

                conn.execute(
                    "UPDATE workspaces SET lifecycle_state = 'archived', archived_at = ?1, updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, ws_id],
                )
                .map_err(|e| format!("Failed to archive workspace: {}", e))?;
                applied_systems.push("workspaces".to_string());

                let payload = serde_json::json!({
                    "action": "archive_context",
                    "workspace_id": ws_id,
                    "reason": event.reason.as_deref().unwrap_or("User explicitly archived context"),
                    "recorded_at": now
                })
                .to_string();

                conn.execute(
                    "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
                     VALUES (?1, ?2, NULL, 'context_archived', NULL, ?3, ?4, ?4)",
                    rusqlite::params![signal_id, ws_id, payload, now],
                )
                .map_err(|e| format!("Failed to insert archive signal: {}", e))?;
                applied_systems.push("context_signals".to_string());

                Self::invalidate_caches_for_workspace(Some(ws_id));
                applied_systems.push("cache_invalidation".to_string());

                Ok(FeedbackApplicationResult {
                    event_id: event.id.clone(),
                    action: event.action,
                    scope: event.scope.clone(),
                    applied_systems,
                    signal_id: Some(signal_id),
                    details: format!("Archived context workspace '{}'", ws_id),
                    success: true,
                })
            }
        }
    }

    /// Corrects the task understanding for a workspace context.
    ///
    /// 1. Updates previous task candidate (if specified or currently active) to Dismissed/Inactive
    ///    without deleting it from the database (preserving historical task activity).
    /// 2. Creates or promotes the new task candidate as Confirmed with confidence 1.0.
    /// 3. Emits auditable feedback event, context signal, and task transition.
    /// 4. Future context state, memory, and reconstruction respect this correction.
    pub fn correct_task_candidate(
        &self,
        conn: &rusqlite::Connection,
        workspace_id: &str,
        previous_task_id_or_title: Option<&str>,
        new_task_title: &str,
        source: FeedbackSource,
        reason: Option<&str>,
    ) -> Result<FeedbackApplicationResult, String> {
        let existing_candidates = crate::db::operations::list_task_candidates_for_context(conn, workspace_id)
            .map_err(|e| format!("Failed to list task candidates: {}", e))?;

        let mut prev_candidate_id: Option<String> = None;
        let mut prev_candidate_title: Option<String> = None;

        if let Some(prev) = previous_task_id_or_title {
            for cand in &existing_candidates {
                if cand.id == prev || cand.title.eq_ignore_ascii_case(prev) {
                    prev_candidate_id = Some(cand.id.clone());
                    prev_candidate_title = Some(cand.title.clone());
                    break;
                }
            }
        } else {
            // Find currently active candidate
            for cand in &existing_candidates {
                if cand.status == crate::db::models::TaskCandidateStatus::Active
                    || cand.status == crate::db::models::TaskCandidateStatus::Detected
                    || cand.status == crate::db::models::TaskCandidateStatus::Confirmed
                {
                    prev_candidate_id = Some(cand.id.clone());
                    prev_candidate_title = Some(cand.title.clone());
                    break;
                }
            }
        }

        if let Some(ref prev_id) = prev_candidate_id {
            // Demote previous task candidate to Dismissed so it remains in historical activity without deletion
            crate::db::operations::update_task_candidate_status(
                conn,
                prev_id,
                crate::db::models::TaskCandidateStatus::Dismissed,
            )
            .map_err(|e| format!("Failed to update previous task candidate status: {}", e))?;
        }

        // 2. Find or create the new task candidate
        let matching_new = existing_candidates.iter().find(|c| c.title.eq_ignore_ascii_case(new_task_title));

        let confirmed_task_id = if let Some(cand) = matching_new {
            crate::db::operations::confirm_task_candidate(conn, &cand.id, 1.0)
                .map_err(|e| format!("Failed to confirm task candidate: {}", e))?;
            cand.id.clone()
        } else {
            let new_cand = crate::db::models::TaskCandidate::new(
                Some(workspace_id.to_string()),
                new_task_title,
                1.0,
            )
            .with_status(crate::db::models::TaskCandidateStatus::Confirmed);
            let created = crate::db::operations::create_task_candidate(conn, &new_cand)
                .map_err(|e| format!("Failed to create confirmed task candidate: {}", e))?;
            created.id
        };

        // 3. Record Feedback Event & Apply
        let event = ContextFeedbackEvent::confirm_task(
            confirmed_task_id.clone(),
            workspace_id,
            source,
        )
        .with_reason(reason.unwrap_or(&format!(
            "User corrected task from '{}' to '{}'",
            prev_candidate_title.as_deref().unwrap_or("none"),
            new_task_title
        )));

        let mut app_res = Self::apply_feedback(conn, &event)?;
        app_res.details = format!(
            "Corrected task in workspace '{}' from '{}' to confirmed task '{}'",
            workspace_id,
            prev_candidate_title.as_deref().unwrap_or("none"),
            new_task_title
        );

        Ok(app_res)
    }

    /// Explicitly resumes a context workspace from Paused, Dormant, or Archived to Active.
    ///
    /// 1. Updates workspace lifecycle_state to 'active', clears archived_at, and touches activity timestamp.
    /// 2. Records an auditable context_resumed signal and feedback event.
    /// 3. Invalidates all caches so future reconstruction reflects active state.
    pub fn resume_context(
        &self,
        conn: &rusqlite::Connection,
        workspace_id: &str,
        source: FeedbackSource,
    ) -> Result<FeedbackApplicationResult, String> {
        let event = ContextFeedbackEvent::resume_context(workspace_id, source);
        Self::apply_feedback(conn, &event)
    }

    /// Explicitly archives a context workspace.
    ///
    /// 1. Updates workspace lifecycle_state to 'archived' and sets archived_at timestamp.
    /// 2. Records an auditable context_archived signal and feedback event.
    /// 3. Invariant: Does NOT delete items, task candidates, relationships, or history (Archived != Deleted).
    /// 4. Invalidates caches and suppresses automated matching.
    pub fn archive_context(
        &self,
        conn: &rusqlite::Connection,
        workspace_id: &str,
        source: FeedbackSource,
        reason: Option<&str>,
    ) -> Result<FeedbackApplicationResult, String> {
        let event = ContextFeedbackEvent::archive_context(workspace_id, source, reason.map(|s| s.to_string()));
        Self::apply_feedback(conn, &event)
    }

    /// Explicitly restores / unarchives an archived context workspace back to active.
    pub fn restore_context(
        &self,
        conn: &rusqlite::Connection,
        workspace_id: &str,
        source: FeedbackSource,
    ) -> Result<FeedbackApplicationResult, String> {
        self.resume_context(conn, workspace_id, source)
    }

    /// Helper to invalidate all relevant downstream contextual caches for a workspace.
    fn invalidate_caches_for_workspace(workspace_id: Option<&str>) {
        crate::context_engine::workset::invalidate_workset_cache(workspace_id);
        crate::context_engine::memory::invalidate_memory_cache(workspace_id);
        crate::context_engine::reconstruction::invalidate_reconstruction_cache(workspace_id);
        if let Some(ws) = workspace_id {
            crate::context_engine::snapshot::invalidate_snapshot_cache(ws);
        }
    }
}

// =========================================================================
// Contextual Negative Feedback Engine (Step 5)
// =========================================================================

/// Structured verdict produced when evaluating contextual negative feedback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NegativeFeedbackVerdict {
    /// No negative feedback exists for this item/context pair; association allowed.
    Allowed,
    /// Negative feedback actively discourages this association with a calculated penalty.
    Discouraged {
        penalty: f32,
        rejection_count: usize,
        reason: String,
    },
    /// Prior negative feedback exists, but was explicitly overridden by a later user confirmation.
    OverriddenByExplicitConfirmation {
        confirmed_at: String,
        reason: String,
    },
    /// Prior negative feedback exists, but future strong corroborated evidence overrides it.
    OverriddenByStrongEvidence {
        confidence: f64,
        reason: String,
    },
}

impl NegativeFeedbackVerdict {
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            Self::Allowed
                | Self::OverriddenByExplicitConfirmation { .. }
                | Self::OverriddenByStrongEvidence { .. }
        )
    }

    pub fn is_discouraged(&self) -> bool {
        matches!(self, Self::Discouraged { .. })
    }
}

/// Comprehensive assessment of negative feedback for an item in a specific context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NegativeFeedbackAssessment {
    /// Target item identifier
    pub item_id: String,
    /// Target context/workspace identifier
    pub context_id: String,
    /// Total count of explicit rejections recorded for this item in this context
    pub rejection_count: usize,
    /// Base penalty before time decay or override
    pub base_penalty: f32,
    /// Effective penalty discount applied to score [0.0, 1.0] after time decay
    pub effective_penalty: f32,
    /// Whether future association is currently discouraged
    pub is_discouraged: bool,
    /// Whether prior negative feedback was overridden by explicit confirmation or strong evidence
    pub is_overridden: bool,
    /// Whether negative feedback has decayed due to age (>= 60 days)
    pub is_stale: bool,
    /// Timestamp of the most recent rejection
    pub latest_rejection_at: Option<String>,
    /// Timestamp of the most recent confirmation
    pub latest_confirmation_at: Option<String>,
    /// Structured verdict
    pub verdict: NegativeFeedbackVerdict,
}

/// Evaluator responsible for computing contextual negative feedback penalties,
/// resolving feedback conflicts, evaluating repeated rejections, and decaying stale rejections.
pub struct ContextNegativeFeedbackEvaluator;

impl ContextNegativeFeedbackEvaluator {
    /// Evaluates negative feedback against the current timestamp.
    pub fn evaluate(
        conn: &rusqlite::Connection,
        item_id: &str,
        context_id: &str,
        candidate_confidence: f64,
        has_strong_anchor: bool,
        is_explicit_user_action: bool,
    ) -> NegativeFeedbackAssessment {
        Self::evaluate_with_time(
            conn,
            item_id,
            context_id,
            candidate_confidence,
            has_strong_anchor,
            is_explicit_user_action,
            Utc::now(),
        )
    }

    /// Evaluates negative feedback against an explicit evaluation timestamp (enabling deterministic testing of time decay).
    pub fn evaluate_with_time(
        conn: &rusqlite::Connection,
        item_id: &str,
        context_id: &str,
        candidate_confidence: f64,
        has_strong_anchor: bool,
        is_explicit_user_action: bool,
        current_time: DateTime<Utc>,
    ) -> NegativeFeedbackAssessment {
        // 1. Authoritative explicit user action overrides all prior rejections immediately
        if is_explicit_user_action {
            return NegativeFeedbackAssessment {
                item_id: item_id.to_string(),
                context_id: context_id.to_string(),
                rejection_count: 0,
                base_penalty: 0.0,
                effective_penalty: 0.0,
                is_discouraged: false,
                is_overridden: true,
                is_stale: false,
                latest_rejection_at: None,
                latest_confirmation_at: Some(current_time.to_rfc3339()),
                verdict: NegativeFeedbackVerdict::OverriddenByExplicitConfirmation {
                    confirmed_at: current_time.to_rfc3339(),
                    reason: "Explicit user action overrides prior negative feedback".to_string(),
                },
            };
        }

        // 2. Fetch rejection history for this exact (item_id, context_id) pair from context_signals
        let mut rejection_timestamps: Vec<DateTime<Utc>> = Vec::new();
        let mut confirmation_timestamps: Vec<DateTime<Utc>> = Vec::new();

        if let Ok(mut stmt) = conn.prepare(
            "SELECT signal_type, created_at FROM context_signals
             WHERE workspace_id = ?1 AND item_id = ?2
             ORDER BY created_at ASC",
        ) {
            let rows = stmt.query_map(rusqlite::params![context_id, item_id], |row| {
                let sig_type: String = row.get(0)?;
                let created_at_str: String = row.get(1)?;
                Ok((sig_type, created_at_str))
            });

            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    let (sig_type, created_str) = r;
                    if let Ok(dt) = DateTime::parse_from_rfc3339(&created_str) {
                        let utc_dt = dt.with_timezone(&Utc);
                        if sig_type == "user_correction"
                            || sig_type == "membership_dismissed"
                            || sig_type == "user_dismiss"
                            || sig_type == "suggestion_dismissed"
                            || sig_type == "user_removed_from_context"
                        {
                            rejection_timestamps.push(utc_dt);
                        } else if sig_type == "user_feedback"
                            || sig_type == "context_confirmed"
                            || sig_type == "confirm_context"
                        {
                            confirmation_timestamps.push(utc_dt);
                        }
                    }
                }
            }
        }

        // Fallback to inspect context_memberships table only if no signals exist
        if rejection_timestamps.is_empty() || confirmation_timestamps.is_empty() {
            let mem_row: rusqlite::Result<(String, String)> = conn.query_row(
                "SELECT membership_type, updated_at FROM context_memberships
                 WHERE item_id = ?1 AND workspace_id = ?2",
                rusqlite::params![item_id, context_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            );

            if let Ok((mem_type, updated_str)) = mem_row {
                if let Ok(dt) = DateTime::parse_from_rfc3339(&updated_str) {
                    let utc_dt = dt.with_timezone(&Utc);
                    if mem_type == "dismissed" && rejection_timestamps.is_empty() {
                        rejection_timestamps.push(utc_dt);
                    } else if mem_type == "confirmed" && confirmation_timestamps.is_empty() {
                        confirmation_timestamps.push(utc_dt);
                    }
                }
            }
        }

        let latest_rejection = rejection_timestamps.iter().max().copied();
        let latest_confirmation = confirmation_timestamps.iter().max().copied();

        // 3. If no rejection exists for this context -> Allowed
        if rejection_timestamps.is_empty() {
            return NegativeFeedbackAssessment {
                item_id: item_id.to_string(),
                context_id: context_id.to_string(),
                rejection_count: 0,
                base_penalty: 0.0,
                effective_penalty: 0.0,
                is_discouraged: false,
                is_overridden: false,
                is_stale: false,
                latest_rejection_at: None,
                latest_confirmation_at: latest_confirmation.map(|t| t.to_rfc3339()),
                verdict: NegativeFeedbackVerdict::Allowed,
            };
        }

        let rejection_count = rejection_timestamps.len();
        let latest_rej_dt = latest_rejection.unwrap();
        let latest_rej_str = latest_rej_dt.to_rfc3339();

        // 4. Check for conflicting feedback (Later Confirmation supersedes prior rejection)
        if let Some(latest_conf_dt) = latest_confirmation {
            if latest_conf_dt > latest_rej_dt {
                return NegativeFeedbackAssessment {
                    item_id: item_id.to_string(),
                    context_id: context_id.to_string(),
                    rejection_count,
                    base_penalty: 0.0,
                    effective_penalty: 0.0,
                    is_discouraged: false,
                    is_overridden: true,
                    is_stale: false,
                    latest_rejection_at: Some(latest_rej_str),
                    latest_confirmation_at: Some(latest_conf_dt.to_rfc3339()),
                    verdict: NegativeFeedbackVerdict::OverriddenByExplicitConfirmation {
                        confirmed_at: latest_conf_dt.to_rfc3339(),
                        reason: "Later user confirmation has higher priority over prior rejection".to_string(),
                    },
                };
            }
        }

        // 5. Calculate repeated rejection penalty
        let base_penalty = Self::calculate_repeated_rejection_penalty(rejection_count);

        // 6. Calculate stale feedback decay (30-day half life)
        let (effective_penalty, is_stale) =
            Self::calculate_decayed_penalty(base_penalty, latest_rej_dt, current_time, 30.0);

        // 7. Check if future strong corroborated evidence overrides it
        if has_strong_anchor && candidate_confidence >= 0.85 {
            return NegativeFeedbackAssessment {
                item_id: item_id.to_string(),
                context_id: context_id.to_string(),
                rejection_count,
                base_penalty,
                effective_penalty: 0.0,
                is_discouraged: false,
                is_overridden: true,
                is_stale,
                latest_rejection_at: Some(latest_rej_str),
                latest_confirmation_at: latest_confirmation.map(|t| t.to_rfc3339()),
                verdict: NegativeFeedbackVerdict::OverriddenByStrongEvidence {
                    confidence: candidate_confidence,
                    reason: format!(
                        "Strong corroborated anchor (confidence {:.1}%) legitimately overrides prior rejection",
                        candidate_confidence * 100.0
                    ),
                },
            };
        }

        // 8. Otherwise, association is discouraged
        NegativeFeedbackAssessment {
            item_id: item_id.to_string(),
            context_id: context_id.to_string(),
            rejection_count,
            base_penalty,
            effective_penalty,
            is_discouraged: effective_penalty > 0.01,
            is_overridden: false,
            is_stale,
            latest_rejection_at: Some(latest_rej_str),
            latest_confirmation_at: latest_confirmation.map(|t| t.to_rfc3339()),
            verdict: NegativeFeedbackVerdict::Discouraged {
                penalty: effective_penalty,
                rejection_count,
                reason: format!(
                    "Item was rejected {} time(s) from workspace '{}' (effective penalty: {:.2})",
                    rejection_count, context_id, effective_penalty
                ),
            },
        }
    }

    /// Computes cumulative penalty for repeated rejections:
    /// 1 rejection: 0.40, 2 rejections: 0.65, 3 rejections: 0.80, 4+ rejections: 0.90 (max 0.95)
    pub fn calculate_repeated_rejection_penalty(rejection_count: usize) -> f32 {
        match rejection_count {
            0 => 0.0,
            1 => 0.40,
            2 => 0.65,
            3 => 0.80,
            _ => 0.90_f32.min(0.95),
        }
    }

    /// Computes exponential decay for negative feedback over time using a half-life model.
    /// Returns `(decayed_penalty, is_stale)`.
    pub fn calculate_decayed_penalty(
        base_penalty: f32,
        rejection_time: DateTime<Utc>,
        current_time: DateTime<Utc>,
        half_life_days: f64,
    ) -> (f32, bool) {
        if current_time <= rejection_time {
            return (base_penalty, false);
        }

        let elapsed_seconds = (current_time - rejection_time).num_seconds() as f64;
        let elapsed_days = (elapsed_seconds / 86400.0).max(0.0);
        let decay_factor = 0.5_f64.powf(elapsed_days / half_life_days);
        let decayed_penalty = (base_penalty as f64 * decay_factor) as f32;
        let is_stale = elapsed_days >= 60.0;

        (decayed_penalty.clamp(0.0, 1.0), is_stale)
    }
}

// =========================================================================
// Contextual Positive Feedback & Relationship Strengthening Engine (Step 6)
// =========================================================================

/// Structured assessment of positive contextual feedback and reinforced relationships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PositiveFeedbackAssessment {
    /// Entity ID evaluated
    pub entity_id: String,
    /// Context / workspace ID evaluated
    pub context_id: String,
    /// Whether this entity has an explicit user confirmation in this context
    pub is_confirmed: bool,
    /// Calibrated confidence level [0.0, 1.0] (1.0 for explicit user confirmation)
    pub confidence: f64,
    /// Direct relationship strength [0.0, 1.0]
    pub relationship_strength: f64,
    /// Whether this entity is marked as an important memory in context
    pub is_important_memory: bool,
    /// Whether this entity is an active/confirmed task candidate in context
    pub is_active_task: bool,
    /// Evidence localization flag: confirms that evidence is strictly scoped
    pub is_localized: bool,
    /// Timestamp when confirmation was registered (RFC 3339 format)
    pub confirmed_at: Option<String>,
    /// Human-readable explanation of relationship support
    pub explanation: String,
}

/// Verification result demonstrating evidence localization between confirmed and related items.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelationshipLocalizationAssessment {
    /// Target confirmed entity ID
    pub confirmed_entity_id: String,
    /// Adjacent / related entity ID (e.g. linked webpage or repo file)
    pub related_entity_id: String,
    /// Context ID evaluated
    pub context_id: String,
    /// Confirmed entity is explicitly supported in context
    pub confirmed_entity_supported: bool,
    /// Related entity is NOT blindly promoted or automatically confirmed
    pub related_entity_blindly_promoted: bool,
    /// Whether strict localization invariant holds
    pub is_isolated: bool,
    /// Diagnostic summary
    pub summary: String,
}

/// Service that evaluates confirmed contextual relationships, positive feedback strength,
/// and validates evidence localization across adjacent entities.
pub struct ContextPositiveFeedbackEngine;

impl ContextPositiveFeedbackEngine {
    /// Evaluates confirmed positive relationship support for an entity in a workspace context.
    pub fn evaluate_confirmed_relationship(
        conn: &rusqlite::Connection,
        entity_id: &str,
        context_id: &str,
    ) -> PositiveFeedbackAssessment {
        // 1. Check direct context_memberships
        let membership_opt: Option<(String, f64, String, String)> = conn
            .query_row(
                "SELECT membership_type, confidence, source, updated_at FROM context_memberships
                 WHERE item_id = ?1 AND workspace_id = ?2",
                rusqlite::params![entity_id, context_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();

        // 2. Check items physical assignment
        let item_ws_id: Option<Option<String>> = conn
            .query_row(
                "SELECT workspace_id FROM items WHERE id = ?1",
                rusqlite::params![entity_id],
                |r| Ok(r.get(0)?),
            )
            .ok();

        // 3. Check important memory status from context_memberships or context_signals
        let is_important = conn
            .query_row(
                "SELECT COUNT(*) FROM context_memberships
                 WHERE workspace_id = ?1 AND item_id = ?2 AND membership_type = 'confirmed'",
                rusqlite::params![context_id, entity_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| cnt > 0)
            .unwrap_or(false)
            || conn
                .query_row(
                    "SELECT COUNT(*) FROM context_signals
                     WHERE workspace_id = ?1 AND item_id = ?2 AND (signal_type = 'important_memory_control' OR payload LIKE '%keep_important%' OR payload LIKE '%mark_important%')",
                    rusqlite::params![context_id, entity_id],
                    |r| r.get::<_, i64>(0),
                )
                .map(|cnt| cnt > 0)
                .unwrap_or(false);

        // 4. Check task candidate status
        let is_task_active = conn
            .query_row(
                "SELECT COUNT(*) FROM task_candidates
                 WHERE id = ?1 AND (context_id = ?2 OR context_id IS NULL) AND status IN ('active', 'confirmed')",
                rusqlite::params![entity_id, context_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| cnt > 0)
            .unwrap_or(false);

        let mut is_confirmed = false;
        let mut confidence = 0.0;
        let mut confirmed_at = None;
        let mut explanation = format!("No positive confirmation for entity '{}' in context '{}'", entity_id, context_id);

        if let Some((mem_type, conf, source, updated_at)) = membership_opt {
            if mem_type == "confirmed" && (source == "user" || conf >= 0.95) {
                is_confirmed = true;
                confidence = 1.0;
                confirmed_at = Some(updated_at);
                explanation = format!(
                    "Explicitly confirmed association with workspace '{}' (confidence: 1.0, source: '{}')",
                    context_id, source
                );
            } else if mem_type == "suggested" {
                confidence = conf;
                explanation = format!("Inferred suggestion for workspace '{}' (confidence: {:.2})", context_id, conf);
            }
        } else if item_ws_id == Some(Some(context_id.to_string())) {
            is_confirmed = true;
            confidence = 1.0;
            explanation = format!("Item directly filed under workspace '{}'", context_id);
        } else if is_task_active {
            is_confirmed = true;
            confidence = 1.0;
            explanation = format!("Active confirmed task candidate in workspace '{}'", context_id);
        }

        PositiveFeedbackAssessment {
            entity_id: entity_id.to_string(),
            context_id: context_id.to_string(),
            is_confirmed,
            confidence,
            relationship_strength: if is_confirmed { 1.0 } else { confidence },
            is_important_memory: is_important,
            is_active_task: is_task_active,
            is_localized: true,
            confirmed_at,
            explanation,
        }
    }

    /// Verifies that positive feedback on `confirmed_entity_id` is strictly localized and does NOT
    /// blindly promote or cascade to adjacent / related entities without independent corroboration.
    pub fn verify_relationship_localization(
        conn: &rusqlite::Connection,
        confirmed_entity_id: &str,
        related_entity_id: &str,
        context_id: &str,
    ) -> RelationshipLocalizationAssessment {
        let confirmed_assessment = Self::evaluate_confirmed_relationship(conn, confirmed_entity_id, context_id);
        let related_assessment = Self::evaluate_confirmed_relationship(conn, related_entity_id, context_id);

        let confirmed_supported = confirmed_assessment.is_confirmed;
        let related_blindly_promoted = related_assessment.is_confirmed;
        let is_isolated = confirmed_supported && !related_blindly_promoted;

        RelationshipLocalizationAssessment {
            confirmed_entity_id: confirmed_entity_id.to_string(),
            related_entity_id: related_entity_id.to_string(),
            context_id: context_id.to_string(),
            confirmed_entity_supported: confirmed_supported,
            related_entity_blindly_promoted: related_blindly_promoted,
            is_isolated,
            summary: if is_isolated {
                format!(
                    "Positive feedback on '{}' is strictly localized; related entity '{}' remains unpromoted in '{}'",
                    confirmed_entity_id, related_entity_id, context_id
                )
            } else {
                format!(
                    "Localization check: confirmed_supported={}, related_blindly_promoted={}",
                    confirmed_supported, related_blindly_promoted
                )
            },
        }
    }
}

// =========================================================================
// Contextual Feedback & Evidence Precedence Engine (Step 7)
// =========================================================================

/// 7-Tier hierarchy defining deterministic authority and priority among
/// explicit user actions and inferred contextual evidence.
///
/// Precedence Hierarchy (Tier 1 is highest priority):
/// 1. `ExplicitUserConfirmation` — Authoritative ground truth (manual assignment, confirm action, pinned memory).
/// 2. `ExplicitUserRejection` — Explicit user suppression/correction (overrides inference, decays with time).
/// 3. `StrongContextualEvidence` — Corroborated multi-factor anchors (confidence >= 0.85).
/// 4. `InferredSemanticSimilarity` — Embedding vector cosine score & topic overlap.
/// 5. `WeakContextualEvidence` — Single-factor, partial, or uncorroborated hints.
/// 6. `TemporalProximity` — Burst proximity within same active window.
/// 7. `ApplicationIdentity` — Multi-purpose host application name alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePriorityTier {
    /// Tier 1: Ground-truth affirmative user action or confirmed assignment
    ExplicitUserConfirmation,
    /// Tier 2: Explicit negative user feedback or correction
    ExplicitUserRejection,
    /// Tier 3: Strong corroborated contextual evidence anchor (>= 0.85)
    StrongContextualEvidence,
    /// Tier 4: Inferred semantic similarity (embeddings/topic models)
    InferredSemanticSimilarity,
    /// Tier 5: Weak, partial, or uncorroborated indicators
    WeakContextualEvidence,
    /// Tier 6: Transient temporal proximity or burst co-occurrence
    TemporalProximity,
    /// Tier 7: Host application identity alone (Chrome, VS Code)
    ApplicationIdentity,
}

impl EvidencePriorityTier {
    /// Numeric rank (1 is highest priority, 7 is lowest priority).
    pub fn rank(&self) -> u8 {
        match self {
            Self::ExplicitUserConfirmation => 1,
            Self::ExplicitUserRejection => 2,
            Self::StrongContextualEvidence => 3,
            Self::InferredSemanticSimilarity => 4,
            Self::WeakContextualEvidence => 5,
            Self::TemporalProximity => 6,
            Self::ApplicationIdentity => 7,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExplicitUserConfirmation => "explicit_user_confirmation",
            Self::ExplicitUserRejection => "explicit_user_rejection",
            Self::StrongContextualEvidence => "strong_contextual_evidence",
            Self::InferredSemanticSimilarity => "inferred_semantic_similarity",
            Self::WeakContextualEvidence => "weak_contextual_evidence",
            Self::TemporalProximity => "temporal_proximity",
            Self::ApplicationIdentity => "application_identity",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ExplicitUserConfirmation => "Tier 1: Explicit User Confirmation",
            Self::ExplicitUserRejection => "Tier 2: Explicit User Rejection",
            Self::StrongContextualEvidence => "Tier 3: Strong Contextual Evidence",
            Self::InferredSemanticSimilarity => "Tier 4: Inferred Semantic Similarity",
            Self::WeakContextualEvidence => "Tier 5: Weak Contextual Evidence",
            Self::TemporalProximity => "Tier 6: Temporal Proximity",
            Self::ApplicationIdentity => "Tier 7: Application Identity",
        }
    }

    /// Whether this tier represents an authoritative explicit user action.
    pub fn is_explicit_user_action(&self) -> bool {
        matches!(self, Self::ExplicitUserConfirmation | Self::ExplicitUserRejection)
    }

    /// Returns true if this tier strictly dominates `other` in the priority hierarchy.
    pub fn has_higher_precedence_than(&self, other: &Self) -> bool {
        self.rank() < other.rank()
    }
}

impl PartialOrd for EvidencePriorityTier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EvidencePriorityTier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Lower rank number means higher priority
        other.rank().cmp(&self.rank())
    }
}

/// Structured prioritized signal used for deterministic multi-source precedence resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrioritizedEvidenceSignal {
    pub signal_id: String,
    pub tier: EvidencePriorityTier,
    pub context_id: Option<String>,
    pub entity_id: Option<String>,
    pub strength: f64,
    pub timestamp: String,
    pub description: String,
    pub is_explicit_user_action: bool,
}

impl PrioritizedEvidenceSignal {
    pub fn new(
        signal_id: impl Into<String>,
        tier: EvidencePriorityTier,
        context_id: Option<String>,
        entity_id: Option<String>,
        strength: f64,
        timestamp: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let is_user = tier.is_explicit_user_action();
        Self {
            signal_id: signal_id.into(),
            tier,
            context_id,
            entity_id,
            strength,
            timestamp: timestamp.into(),
            description: description.into(),
            is_explicit_user_action: is_user,
        }
    }

    pub fn timestamp_dt(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.timestamp).ok().map(|dt| dt.with_timezone(&Utc))
    }
}

/// Policy configuration governing deterministic feedback precedence evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackPrecedencePolicy {
    /// Minimum confidence threshold for strong evidence override (default: 0.85)
    pub strong_evidence_override_threshold: f64,
    /// Whether explicit user confirmation unconditionally overrides prior rejections (default: true)
    pub explicit_confirmation_dominates: bool,
    /// Days before negative feedback is considered stale (default: 60.0)
    pub stale_rejection_decay_days: f64,
}

impl Default for FeedbackPrecedencePolicy {
    fn default() -> Self {
        Self {
            strong_evidence_override_threshold: 0.85,
            explicit_confirmation_dominates: true,
            stale_rejection_decay_days: 60.0,
        }
    }
}

/// Result of resolving precedence among conflicting or competing contextual signals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrecedenceResolutionResult {
    pub winning_signal: PrioritizedEvidenceSignal,
    pub winning_tier: EvidencePriorityTier,
    pub effective_verdict: String,
    pub overridden_signals: Vec<PrioritizedEvidenceSignal>,
    pub explanation: String,
}

/// Dedicated precedence engine that deterministically arbitrates between conflicting user feedback
/// and inferred contextual signals based on explicit priority rules.
pub struct FeedbackPrecedenceEngine;

impl FeedbackPrecedenceEngine {
    /// Classifies an evidence item into its corresponding priority tier.
    pub fn classify_evidence_tier(
        category: &crate::context_engine::evidence::EvidenceCategory,
        confidence: f64,
        has_strong_anchor: bool,
    ) -> EvidencePriorityTier {
        use crate::context_engine::evidence::EvidenceCategory;
        match category {
            EvidenceCategory::ConfirmedMembership => EvidencePriorityTier::ExplicitUserConfirmation,
            EvidenceCategory::UserFeedback => EvidencePriorityTier::ExplicitUserRejection,
            EvidenceCategory::BrowserDomain | EvidenceCategory::BrowserUrl | EvidenceCategory::SameSession | EvidenceCategory::RelatedItem => {
                if has_strong_anchor && confidence >= 0.85 {
                    EvidencePriorityTier::StrongContextualEvidence
                } else if confidence >= 0.40 {
                    EvidencePriorityTier::InferredSemanticSimilarity
                } else {
                    EvidencePriorityTier::WeakContextualEvidence
                }
            }
            EvidenceCategory::SemanticSimilarity | EvidenceCategory::WorkspaceHistory => {
                if has_strong_anchor && confidence >= 0.85 {
                    EvidencePriorityTier::StrongContextualEvidence
                } else {
                    EvidencePriorityTier::InferredSemanticSimilarity
                }
            }
            EvidenceCategory::TemporalProximity => EvidencePriorityTier::TemporalProximity,
            EvidenceCategory::SourceApp | EvidenceCategory::CaptureMethod | EvidenceCategory::ContentType | EvidenceCategory::FileMetadata | EvidenceCategory::SourceWindow => {
                EvidencePriorityTier::ApplicationIdentity
            }
        }
    }

    /// Classifies a user feedback action into its corresponding priority tier.
    pub fn classify_feedback_action_tier(action: FeedbackAction) -> EvidencePriorityTier {
        match action {
            FeedbackAction::ConfirmContext
            | FeedbackAction::MarkImportant
            | FeedbackAction::ConfirmTask
            | FeedbackAction::ResumeContext => EvidencePriorityTier::ExplicitUserConfirmation,

            FeedbackAction::RejectContext
            | FeedbackAction::RemoveFromContext
            | FeedbackAction::RemoveImportance
            | FeedbackAction::RejectTask
            | FeedbackAction::DismissSuggestion
            | FeedbackAction::ArchiveContext => EvidencePriorityTier::ExplicitUserRejection,
        }
    }

    /// Compares two signals deterministically by tier rank, timestamp freshness, and strength.
    pub fn compare_precedence(
        sig_a: &PrioritizedEvidenceSignal,
        sig_b: &PrioritizedEvidenceSignal,
    ) -> std::cmp::Ordering {
        // 1. Tier comparison: lower rank number = higher precedence
        let tier_ord = sig_a.tier.rank().cmp(&sig_b.tier.rank());
        if tier_ord != std::cmp::Ordering::Equal {
            // sig_a has higher precedence if rank is smaller (Less)
            return tier_ord.reverse(); // Reverse so Ordering::Greater means sig_a wins
        }

        // 2. Same tier: compare timestamps (newer timestamp wins)
        let ts_a = sig_a.timestamp_dt();
        let ts_b = sig_b.timestamp_dt();
        if let (Some(a), Some(b)) = (ts_a, ts_b) {
            let ts_ord = a.cmp(&b);
            if ts_ord != std::cmp::Ordering::Equal {
                return ts_ord;
            }
        }

        // 3. Fallback: compare strength
        sig_a.strength.partial_cmp(&sig_b.strength).unwrap_or(std::cmp::Ordering::Equal)
    }

    /// Resolves precedence across a collection of candidate evidence signals for a context.
    pub fn resolve_conflicting_signals(
        signals: &[PrioritizedEvidenceSignal],
        _policy: &FeedbackPrecedencePolicy,
    ) -> Result<PrecedenceResolutionResult, String> {
        if signals.is_empty() {
            return Err("Cannot resolve precedence on empty signals list".to_string());
        }

        // Sort signals by precedence descending (highest priority first)
        let mut sorted = signals.to_vec();
        sorted.sort_by(|a, b| Self::compare_precedence(b, a));

        let winner = sorted[0].clone();
        let mut overridden = Vec::new();
        for sig in sorted.iter().skip(1) {
            overridden.push(sig.clone());
        }

        let explanation = format!(
            "Signal '{}' ({}) selected as authoritative with precedence over {} other candidate signal(s)",
            winner.signal_id,
            winner.tier.display_name(),
            overridden.len()
        );

        let verdict = match winner.tier {
            EvidencePriorityTier::ExplicitUserConfirmation => "confirmed_by_user",
            EvidencePriorityTier::ExplicitUserRejection => "rejected_by_user",
            EvidencePriorityTier::StrongContextualEvidence => "strong_corroborated_anchor",
            EvidencePriorityTier::InferredSemanticSimilarity => "inferred_semantic_association",
            EvidencePriorityTier::WeakContextualEvidence => "weak_association",
            EvidencePriorityTier::TemporalProximity => "temporal_proximity_association",
            EvidencePriorityTier::ApplicationIdentity => "app_identity_association",
        }
        .to_string();

        Ok(PrecedenceResolutionResult {
            winning_signal: winner.clone(),
            winning_tier: winner.tier,
            effective_verdict: verdict,
            overridden_signals: overridden,
            explanation,
        })
    }
}

// =========================================================================
// Contextual Feedback Conflict Resolver (Step 8)
// =========================================================================

/// Derived effective state for an entity in a context after deterministic conflict resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveFeedbackState {
    /// Explicitly confirmed by latest authoritative user feedback (affirmative).
    Confirmed {
        confidence: f64,
        confirmed_at: String,
        event_id: String,
    },
    /// Explicitly rejected or removed by latest user feedback (penalized/suppressed).
    Rejected {
        penalty: f32,
        rejected_at: String,
        rejection_count: usize,
        event_id: String,
    },
    /// Genuinely ambiguous or contradictory feedback without clear timestamp/scope resolution.
    Ambiguous {
        reason: String,
        ambiguity_level: f32,
        competing_events_count: usize,
    },
    /// Neutral state with no recorded explicit user corrections.
    Neutral,
}

impl EffectiveFeedbackState {
    pub fn is_confirmed(&self) -> bool {
        matches!(self, Self::Confirmed { .. })
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected { .. })
    }

    pub fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous { .. })
    }

    pub fn is_neutral(&self) -> bool {
        matches!(self, Self::Neutral)
    }
}

/// Comprehensive result of arbitrating historical feedback conflicts for an entity in a context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackConflictResolution {
    pub target_id: String,
    pub context_id: String,
    pub effective_state: EffectiveFeedbackState,
    pub total_history_count: usize,
    pub confirmations_count: usize,
    pub rejections_count: usize,
    pub latest_action: Option<FeedbackAction>,
    pub latest_timestamp: Option<String>,
    pub is_ambiguous: bool,
    pub history_preserved: bool,
    pub explanation: String,
}

/// Resolver that derives the current effective contextual state from historical feedback events
/// without mutating or deleting historical records.
pub struct FeedbackConflictResolver;

impl FeedbackConflictResolver {
    /// Resolves conflicting feedback events for a given target entity and context.
    ///
    /// Arbitration principles:
    /// 1. **Zero History Deletion**: Preserves the complete event trajectory for auditability.
    /// 2. **Scope Specificity**: Narrower scopes (e.g. `ItemContext` specificity 1) supersede wider scopes (e.g. `SourceContext` specificity 2 or `Global`).
    /// 3. **Recency**: The newest explicit action governs direction when scopes are comparable.
    /// 4. **No Fabricated Certainty**: Genuinely ambiguous simultaneous conflicts produce `EffectiveFeedbackState::Ambiguous`.
    pub fn resolve_events(
        events: &[ContextFeedbackEvent],
        target_id: &str,
        context_id: &str,
    ) -> FeedbackConflictResolution {
        // Filter events applicable to (target_id, context_id)
        let mut applicable: Vec<&ContextFeedbackEvent> = events
            .iter()
            .filter(|ev| {
                let target_matches = ev.target_ref.id() == target_id
                    || ev.scope.applies_to(Some(target_id), None, None, context_id);
                let context_matches = ev
                    .context_ref
                    .as_deref()
                    .or_else(|| ev.scope.context_id())
                    .map(|c| c == context_id)
                    .unwrap_or(ev.scope == FeedbackScope::Global);
                target_matches && context_matches
            })
            .collect();

        if applicable.is_empty() {
            return FeedbackConflictResolution {
                target_id: target_id.to_string(),
                context_id: context_id.to_string(),
                effective_state: EffectiveFeedbackState::Neutral,
                total_history_count: 0,
                confirmations_count: 0,
                rejections_count: 0,
                latest_action: None,
                latest_timestamp: None,
                is_ambiguous: false,
                history_preserved: true,
                explanation: format!("No feedback history recorded for target '{}' in context '{}'", target_id, context_id),
            };
        }

        // Sort events chronologically, then by specificity (narrower scope first)
        applicable.sort_by(|a, b| {
            let ts_ord = match (a.timestamp_dt(), b.timestamp_dt()) {
                (Some(ta), Some(tb)) => ta.cmp(&tb),
                _ => a.timestamp.cmp(&b.timestamp),
            };
            if ts_ord != std::cmp::Ordering::Equal {
                ts_ord
            } else {
                // Same timestamp: narrower specificity (lower number) comes later (higher priority)
                b.scope.specificity_order().cmp(&a.scope.specificity_order())
            }
        });

        let total_history_count = applicable.len();
        let mut confirmations_count = 0;
        let mut rejections_count = 0;

        for ev in &applicable {
            if ev.action.is_positive() {
                confirmations_count += 1;
            } else if ev.action.is_negative() {
                rejections_count += 1;
            }
        }

        // Check for genuine ambiguity: conflicting simultaneous actions with identical timestamp & specificity
        if applicable.len() >= 2 {
            let last = applicable[applicable.len() - 1];
            let second_last = applicable[applicable.len() - 2];
            if last.timestamp == second_last.timestamp
                && last.scope.specificity_order() == second_last.scope.specificity_order()
                && last.action.is_positive() != second_last.action.is_positive()
            {
                return FeedbackConflictResolution {
                    target_id: target_id.to_string(),
                    context_id: context_id.to_string(),
                    effective_state: EffectiveFeedbackState::Ambiguous {
                        reason: "Simultaneous conflicting feedback events of equal specificity registered".to_string(),
                        ambiguity_level: 0.85,
                        competing_events_count: applicable.len(),
                    },
                    total_history_count,
                    confirmations_count,
                    rejections_count,
                    latest_action: Some(last.action),
                    latest_timestamp: Some(last.timestamp.clone()),
                    is_ambiguous: true,
                    history_preserved: true,
                    explanation: "Genuinely ambiguous simultaneous feedback detected; relationship marked uncertain".to_string(),
                };
            }
        }

        // Latest event dictates effective direction
        let latest = applicable[applicable.len() - 1];
        let effective_state = if latest.action.is_positive() {
            EffectiveFeedbackState::Confirmed {
                confidence: 1.0,
                confirmed_at: latest.timestamp.clone(),
                event_id: latest.id.clone(),
            }
        } else {
            let penalty = ContextNegativeFeedbackEvaluator::calculate_repeated_rejection_penalty(rejections_count);
            EffectiveFeedbackState::Rejected {
                penalty,
                rejected_at: latest.timestamp.clone(),
                rejection_count: rejections_count,
                event_id: latest.id.clone(),
            }
        };

        let explanation = format!(
            "Resolved {} feedback event(s) ({} confirm, {} reject). Latest action '{}' at {} determines effective state",
            total_history_count,
            confirmations_count,
            rejections_count,
            latest.action.as_str(),
            latest.timestamp
        );

        FeedbackConflictResolution {
            target_id: target_id.to_string(),
            context_id: context_id.to_string(),
            effective_state,
            total_history_count,
            confirmations_count,
            rejections_count,
            latest_action: Some(latest.action),
            latest_timestamp: Some(latest.timestamp.clone()),
            is_ambiguous: false,
            history_preserved: true,
            explanation,
        }
    }

    /// Resolves feedback conflicts directly from persisted database context_signals.
    pub fn resolve_from_db(
        conn: &rusqlite::Connection,
        target_id: &str,
        context_id: &str,
    ) -> Result<FeedbackConflictResolution, String> {
        let mut events = Vec::new();

        let mut stmt = conn
            .prepare(
                "SELECT id, signal_type, payload, created_at FROM context_signals
                 WHERE workspace_id = ?1 AND item_id = ?2
                 ORDER BY created_at ASC",
            )
            .map_err(|e| format!("Failed to prepare context_signals query: {}", e))?;

        let rows = stmt
            .query_map(rusqlite::params![context_id, target_id], |row| {
                let id: String = row.get(0)?;
                let sig_type: String = row.get(1)?;
                let payload: String = row.get(2)?;
                let created_at: String = row.get(3)?;
                Ok((id, sig_type, payload, created_at))
            })
            .map_err(|e| format!("Query error: {}", e))?;

        for r in rows.flatten() {
            let (id, sig_type, payload_str, created_at) = r;
            let parsed_action = serde_json::from_str::<serde_json::Value>(&payload_str)
                .ok()
                .and_then(|val| {
                    val.get("action")
                        .and_then(|a| a.as_str())
                        .and_then(|s| FeedbackAction::from_str_lenient(s).ok())
                });

            let action_opt = parsed_action.or_else(|| {
                if sig_type == "user_feedback" || sig_type == "context_confirmed" {
                    Some(FeedbackAction::ConfirmContext)
                } else if sig_type == "user_correction"
                    || sig_type == "membership_dismissed"
                    || sig_type == "user_removed_from_context"
                    || sig_type == "user_dismiss"
                    || sig_type == "user_reject"
                {
                    Some(FeedbackAction::RemoveFromContext)
                } else if sig_type == "suggestion_dismissed" {
                    Some(FeedbackAction::DismissSuggestion)
                } else {
                    None
                }
            });

            if let Some(act) = action_opt {
                let ev = ContextFeedbackEvent::new(
                    act,
                    FeedbackTargetRef::Item(target_id.to_string()),
                    Some(context_id.to_string()),
                    FeedbackSource::UserExplicitUi,
                    FeedbackScope::item_context(target_id, context_id),
                    None,
                )
                .with_timestamp(created_at);
                let mut ev = ev;
                ev.id = id;
                events.push(ev);
            }
        }

        Ok(Self::resolve_events(&events, target_id, context_id))
    }
}

// =========================================================================
// Deterministic Contextual Adaptation Engine (Step 13)
// =========================================================================

/// Structured verdict explaining the outcome of deterministic adaptation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaptationVerdict {
    /// Item was directly confirmed by user in context -> 1.0 confidence boost.
    ConfirmedDirectly,
    /// Item possesses specific contextual entity anchors (repo URL, title, project keywords),
    /// bypassing generic application dampening and receiving strong recognition.
    SpecificRelationshipRecognized,
    /// Generic item from an application with repeated rejections in this context is dampened.
    GenericAppDampened,
    /// The specific item itself was directly rejected by user in this context.
    ItemDirectlyRejected,
    /// Historical rejection has decayed with time, applying only mild dampening.
    DecayedHistoricalPenalty,
    /// No adaptation rules triggered; baseline score is preserved.
    UnmodifiedBaseline,
}

/// Comprehensive result of evaluating deterministic adaptation for an item in a context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeterministicAdaptationAssessment {
    pub item_id: String,
    pub context_id: String,
    pub base_score: f32,
    pub adapted_score: f32,
    pub confidence: f64,
    pub adaptation_delta: f32,
    pub is_generic_suppressed: bool,
    pub has_specific_entity_match: bool,
    pub app_rejection_count: usize,
    pub item_rejection_count: usize,
    pub item_confirmation_count: usize,
    pub decay_factor: f32,
    pub verdict: AdaptationVerdict,
    pub explanation: String,
}

/// Engine that performs deterministic contextual adaptation based on structured user feedback
/// without introducing machine learning models, external training pipelines, or fine-tuning.
pub struct ContextDeterministicAdaptationEngine;

impl ContextDeterministicAdaptationEngine {
    /// Computes the time decay factor [0.4, 1.0] for historical feedback.
    ///
    /// Decay schedule:
    /// - <= 30 days: 1.0 (full active effect)
    /// - 30..90 days: Linear decay from 1.0 to 0.6
    /// - > 90 days: 0.4 (historical baseline floor)
    pub fn compute_time_decay(created_at: DateTime<Utc>, eval_time: DateTime<Utc>) -> f32 {
        let elapsed_secs = (eval_time - created_at).num_seconds().max(0);
        let days = elapsed_secs as f64 / 86400.0;

        if days <= 30.0 {
            1.0
        } else if days <= 90.0 {
            (1.0 - 0.4 * ((days - 30.0) / 60.0)) as f32
        } else {
            0.4
        }
    }

    /// Detects if an item possesses specific contextual entity anchors for a workspace
    /// (e.g. matching repo slug, repository URL, project title tokens, or confirmed domain).
    pub fn detect_specific_entity_affinity(
        conn: &rusqlite::Connection,
        item: &crate::db::models::Item,
        workspace: &crate::db::models::Workspace,
    ) -> (bool, f32, String) {
        let ws_name_clean = workspace.name.trim().to_lowercase();
        let ws_tokens: Vec<String> = ws_name_clean
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 3)
            .map(|s| s.to_string())
            .collect();

        // 1. Extract item properties
        let item_title = item.title.as_deref().unwrap_or("").to_lowercase();
        let item_content = item.content.to_lowercase();
        let mut item_url: Option<String> = None;
        let mut item_domain: Option<String> = None;
        let mut item_repo: Option<String> = None;

        if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(&item.metadata) {
            if let Some(u) = meta_json.get("url").and_then(|v| v.as_str()) {
                item_url = Some(u.to_lowercase());
            }
            if let Some(d) = meta_json.get("domain").and_then(|v| v.as_str()) {
                item_domain = Some(d.to_lowercase());
            }
            if let Some(r) = meta_json.get("repo_slug").and_then(|v| v.as_str()) {
                item_repo = Some(r.to_lowercase());
            }
        }

        if item.item_type == "url" && item_url.is_none() {
            item_url = Some(item_content.clone());
        }

        // 2. Check explicit workspace keywords / tokens in URL, title, or repo slug
        for token in &ws_tokens {
            if let Some(ref url) = item_url {
                if url.contains(token) {
                    return (
                        true,
                        0.30,
                        format!("Item URL '{}' contains specific project identifier '{}'", url, token),
                    );
                }
            }
            if let Some(ref repo) = item_repo {
                if repo.contains(token) {
                    return (
                        true,
                        0.35,
                        format!("Repository slug '{}' matches project identifier '{}'", repo, token),
                    );
                }
            }
            if item_title.contains(token) {
                return (
                    true,
                    0.25,
                    format!("Item title '{}' contains specific project keyword '{}'", item_title, token),
                );
            }
        }

        // 3. Check if item domain or URL matches confirmed workspace entities in database
        if let Ok(mut stmt) = conn.prepare(
            "SELECT i.content, i.title, i.metadata FROM items i
             JOIN context_memberships m ON i.id = m.item_id
             WHERE m.workspace_id = ?1 AND m.membership_type = 'confirmed'",
        ) {
            let confirmed_rows = stmt.query_map(rusqlite::params![workspace.id], |row| {
                let content: String = row.get(0)?;
                let title: Option<String> = row.get(1)?;
                let metadata: String = row.get(2)?;
                Ok((content, title, metadata))
            });

            if let Ok(rows) = confirmed_rows {
                for r in rows.flatten() {
                    let (c_content, _c_title, c_meta) = r;
                    if let Ok(c_meta_json) = serde_json::from_str::<serde_json::Value>(&c_meta) {
                        if let Some(c_repo) = c_meta_json.get("repo_slug").and_then(|v| v.as_str()) {
                            if let Some(ref cur_repo) = item_repo {
                                if cur_repo == &c_repo.to_lowercase() {
                                    return (
                                        true,
                                        0.35,
                                        format!("Matches confirmed workspace repository '{}'", c_repo),
                                    );
                                }
                            }
                        }
                        if let Some(c_domain) = c_meta_json.get("domain").and_then(|v| v.as_str()) {
                            if let Some(ref cur_domain) = item_domain {
                                // Exclude generic shared domains from automatic affinity
                                if cur_domain == &c_domain.to_lowercase()
                                    && !Self::is_generic_public_domain(cur_domain)
                                {
                                    return (
                                        true,
                                        0.20,
                                        format!("Shares confirmed workspace domain '{}'", c_domain),
                                    );
                                }
                            }
                        }
                    }
                    if let Some(ref url) = item_url {
                        if c_content.starts_with("http") && c_content.contains(url.as_str()) {
                            return (
                                true,
                                0.30,
                                format!("Matches confirmed workspace URL reference '{}'", c_content),
                            );
                        }
                    }
                }
            }
        }

        (false, 0.0, "No specific entity anchors found".to_string())
    }

    /// Checks if a domain is a generic public web surface rather than a project-specific host.
    pub fn is_generic_public_domain(domain: &str) -> bool {
        let d = domain.to_lowercase();
        d == "google.com"
            || d == "www.google.com"
            || d == "bing.com"
            || d == "duckduckgo.com"
            || d == "search.yahoo.com"
            || d == "reddit.com"
            || d == "twitter.com"
            || d == "x.com"
            || d == "youtube.com"
            || d == "facebook.com"
            || d == "instagram.com"
            || d == "linkedin.com"
            || d == "wikipedia.org"
    }

    /// Queries app-level rejections for a given application in a workspace context.
    pub fn query_app_rejections(
        conn: &rusqlite::Connection,
        app_name: &str,
        context_id: &str,
    ) -> (usize, Option<DateTime<Utc>>) {
        let mut count = 0;
        let mut latest_dt = None;

        if let Ok(mut stmt) = conn.prepare(
            "SELECT payload, created_at FROM context_signals
             WHERE workspace_id = ?1 AND signal_type IN ('user_correction', 'membership_dismissed', 'user_removed_from_context', 'user_dismiss')
             ORDER BY created_at ASC",
        ) {
            let rows = stmt.query_map(rusqlite::params![context_id], |row| {
                let payload: String = row.get(0)?;
                let created_at: String = row.get(1)?;
                Ok((payload, created_at))
            });

            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    let (payload_str, created_str) = r;
                    let matches_app = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                        val.get("evidence_signature")
                            .and_then(|sig| sig.get("app_name"))
                            .and_then(|a| a.as_str())
                            .map(|a| a.eq_ignore_ascii_case(app_name))
                            .unwrap_or(false)
                            || val.get("app_name")
                                .and_then(|a| a.as_str())
                                .map(|a| a.eq_ignore_ascii_case(app_name))
                                .unwrap_or(false)
                    } else {
                        false
                    };

                    if matches_app {
                        count += 1;
                        if let Ok(dt) = DateTime::parse_from_rfc3339(&created_str) {
                            latest_dt = Some(dt.with_timezone(&Utc));
                        }
                    }
                }
            }
        }

        (count, latest_dt)
    }

    /// Evaluates deterministic contextual adaptation for an item against a workspace using the current timestamp.
    pub fn evaluate_adaptation(
        conn: &rusqlite::Connection,
        item: &crate::db::models::Item,
        workspace: &crate::db::models::Workspace,
        base_score: f32,
    ) -> DeterministicAdaptationAssessment {
        Self::evaluate_adaptation_with_time(conn, item, workspace, base_score, Utc::now())
    }

    /// Evaluates deterministic contextual adaptation with an explicit evaluation timestamp.
    pub fn evaluate_adaptation_with_time(
        conn: &rusqlite::Connection,
        item: &crate::db::models::Item,
        workspace: &crate::db::models::Workspace,
        base_score: f32,
        eval_time: DateTime<Utc>,
    ) -> DeterministicAdaptationAssessment {
        let item_id = &item.id;
        let context_id = &workspace.id;

        // 1. Check direct positive confirmation
        let pos_assessment = ContextPositiveFeedbackEngine::evaluate_confirmed_relationship(conn, item_id, context_id);
        if pos_assessment.is_confirmed {
            return DeterministicAdaptationAssessment {
                item_id: item_id.clone(),
                context_id: context_id.clone(),
                base_score,
                adapted_score: 1.0,
                confidence: 1.0,
                adaptation_delta: 1.0 - base_score,
                is_generic_suppressed: false,
                has_specific_entity_match: true,
                app_rejection_count: 0,
                item_rejection_count: 0,
                item_confirmation_count: 1,
                decay_factor: 1.0,
                verdict: AdaptationVerdict::ConfirmedDirectly,
                explanation: format!(
                    "Item '{}' is explicitly confirmed by user in workspace '{}'",
                    item_id, workspace.name
                ),
            };
        }

        // 2. Check direct negative feedback on this specific item
        let neg_assessment = ContextNegativeFeedbackEvaluator::evaluate_with_time(
            conn,
            item_id,
            context_id,
            base_score as f64,
            false,
            false,
            eval_time,
        );

        if neg_assessment.rejection_count > 0 && !neg_assessment.is_overridden {
            let latest_rej_dt = neg_assessment
                .latest_rejection_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc)))
                .unwrap_or(eval_time);

            let decay_factor = Self::compute_time_decay(latest_rej_dt, eval_time);
            let effective_penalty = neg_assessment.base_penalty * decay_factor;
            let adapted_score = (base_score - effective_penalty).max(0.0);

            let verdict = if neg_assessment.is_stale || decay_factor <= 0.6 {
                AdaptationVerdict::DecayedHistoricalPenalty
            } else {
                AdaptationVerdict::ItemDirectlyRejected
            };

            return DeterministicAdaptationAssessment {
                item_id: item_id.clone(),
                context_id: context_id.clone(),
                base_score,
                adapted_score,
                confidence: (base_score as f64 - effective_penalty as f64).max(0.0),
                adaptation_delta: adapted_score - base_score,
                is_generic_suppressed: false,
                has_specific_entity_match: false,
                app_rejection_count: 0,
                item_rejection_count: neg_assessment.rejection_count,
                item_confirmation_count: 0,
                decay_factor,
                verdict,
                explanation: format!(
                    "Item '{}' was directly rejected {} time(s) for workspace '{}' (effective penalty: -{:.2}, decay factor: {:.2})",
                    item_id, neg_assessment.rejection_count, workspace.name, effective_penalty, decay_factor
                ),
            };
        }

        // 3. Specific Entity / Contextual Relationship Recognition
        let (has_specific_match, specific_boost, specific_reason) =
            Self::detect_specific_entity_affinity(conn, item, workspace);

        // Extract app name from item metadata
        let app_name = serde_json::from_str::<serde_json::Value>(&item.metadata)
            .ok()
            .and_then(|val| val.get("app_name").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| {
                if item.item_type == "url" {
                    "Browser".to_string()
                } else {
                    "Application".to_string()
                }
            });

        // 4. Query app-level rejections
        let (app_rejections, latest_app_rejection_dt) =
            Self::query_app_rejections(conn, &app_name, context_id);

        let app_decay = latest_app_rejection_dt
            .map(|dt| Self::compute_time_decay(dt, eval_time))
            .unwrap_or(1.0);

        // 5. Decision Arbitration: Specific Relationship vs Generic App Dampening
        if has_specific_match {
            // Specific entity relationship strongly recognized: bypasses generic app dampening!
            let adapted_score = (base_score + specific_boost).clamp(0.0, 1.0);
            let confidence = (0.85 + (base_score as f64 * 0.15)).min(1.0);

            DeterministicAdaptationAssessment {
                item_id: item_id.clone(),
                context_id: context_id.clone(),
                base_score,
                adapted_score,
                confidence,
                adaptation_delta: adapted_score - base_score,
                is_generic_suppressed: false,
                has_specific_entity_match: true,
                app_rejection_count: app_rejections,
                item_rejection_count: 0,
                item_confirmation_count: 0,
                decay_factor: 1.0,
                verdict: AdaptationVerdict::SpecificRelationshipRecognized,
                explanation: format!(
                    "Specific relationship recognized for workspace '{}': {}. Bypasses any generic '{}' application dampening.",
                    workspace.name, specific_reason, app_name
                ),
            }
        } else if app_rejections > 0 {
            // Generic page from an app with repeated rejections -> apply generic noise dampener
            let dampening_multiplier = if app_rejections == 1 {
                1.0 - 0.35 * app_decay
            } else {
                // 2+ rejections: strong noise dampener
                1.0 - 0.70 * app_decay
            };

            let mut adapted = base_score * dampening_multiplier;
            if app_rejections >= 2 {
                // Hard cap to ensure generic page remains a weak candidate (<= 0.30)
                adapted = adapted.min(0.30);
            }

            let confidence = (base_score as f64 * dampening_multiplier as f64).max(0.0);

            DeterministicAdaptationAssessment {
                item_id: item_id.clone(),
                context_id: context_id.clone(),
                base_score,
                adapted_score: adapted,
                confidence,
                adaptation_delta: adapted - base_score,
                is_generic_suppressed: true,
                has_specific_entity_match: false,
                app_rejection_count: app_rejections,
                item_rejection_count: 0,
                item_confirmation_count: 0,
                decay_factor: app_decay,
                verdict: AdaptationVerdict::GenericAppDampened,
                explanation: format!(
                    "Generic item from '{}' dampened ({} prior generic rejection(s) in workspace '{}', decay: {:.2}). Score reduced from {:.2} to {:.2}.",
                    app_name, app_rejections, workspace.name, app_decay, base_score, adapted
                ),
            }
        } else {
            // Baseline unmodified
            DeterministicAdaptationAssessment {
                item_id: item_id.clone(),
                context_id: context_id.clone(),
                base_score,
                adapted_score: base_score,
                confidence: base_score as f64,
                adaptation_delta: 0.0,
                is_generic_suppressed: false,
                has_specific_entity_match: false,
                app_rejection_count: 0,
                item_rejection_count: 0,
                item_confirmation_count: 0,
                decay_factor: 1.0,
                verdict: AdaptationVerdict::UnmodifiedBaseline,
                explanation: format!(
                    "No prior feedback or specific entity anchors for item '{}' in workspace '{}'. Baseline score preserved.",
                    item_id, workspace.name
                ),
            }
        }
    }
}

// =========================================================================
// Contextual Feedback Relevance Decay & Persistence Policy (Step 14)
// =========================================================================

/// Semantic category defining the longevity, persistence, and decay schedule of user feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackPersistenceCategory {
    /// Explicit structural decisions that remain permanently influential (e.g. ArchiveContext, ResumeContext, MarkImportant).
    PermanentPreference,
    /// High-longevity relationship adjustments that decay gracefully to an immutable historical anchor floor (e.g. RemoveFromContext, RejectContext, ConfirmTask).
    ContextualCorrection,
    /// Transient suppressions with a finite cooldown window (e.g. DismissSuggestion).
    TemporaryDismissal,
    /// Automated or heuristic signals that decay rapidly based on elapsed time.
    HistoricalInferred,
}

impl FeedbackPersistenceCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PermanentPreference => "permanent_preference",
            Self::ContextualCorrection => "contextual_correction",
            Self::TemporaryDismissal => "temporary_dismissal",
            Self::HistoricalInferred => "historical_inferred",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::PermanentPreference => "Permanent Explicit Preference",
            Self::ContextualCorrection => "Contextual Relationship Correction",
            Self::TemporaryDismissal => "Temporary Suggestion Dismissal",
            Self::HistoricalInferred => "Historical Inferred Feedback",
        }
    }

    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::PermanentPreference)
    }

    pub fn is_contextual_correction(&self) -> bool {
        matches!(self, Self::ContextualCorrection)
    }

    pub fn is_temporary(&self) -> bool {
        matches!(self, Self::TemporaryDismissal | Self::HistoricalInferred)
    }
}

/// Configurable policy parameters governing feedback relevance decay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackRelevanceConfig {
    /// Baseline weight for permanent explicit preferences (default: 1.0)
    pub permanent_retention_weight: f32,
    /// Duration in days where contextual corrections operate at full strength (default: 60.0)
    pub contextual_correction_active_days: f64,
    /// Duration in days over which contextual corrections transition to the floor (default: 60.0)
    pub contextual_correction_decay_window_days: f64,
    /// Immutable historical anchor floor for contextual corrections (default: 0.40)
    pub contextual_correction_floor: f32,
    /// Active cooldown duration in days for temporary suggestion dismissals (default: 14.0)
    pub temporary_dismissal_cooldown_days: f64,
    /// Half-life in days for rapid decay of historical inferred feedback (default: 7.0)
    pub historical_inferred_half_life_days: f64,
}

impl Default for FeedbackRelevanceConfig {
    fn default() -> Self {
        Self {
            permanent_retention_weight: 1.0,
            contextual_correction_active_days: 60.0,
            contextual_correction_decay_window_days: 60.0,
            contextual_correction_floor: 0.40,
            temporary_dismissal_cooldown_days: 14.0,
            historical_inferred_half_life_days: 7.0,
        }
    }
}

/// Evaluation assessment of a feedback item's current mathematical relevance and persistence status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackRelevanceAssessment {
    pub action: FeedbackAction,
    pub source: FeedbackSource,
    pub persistence_category: FeedbackPersistenceCategory,
    pub base_weight: f32,
    pub effective_weight: f32,
    pub elapsed_days: f64,
    pub decay_factor: f32,
    pub is_active: bool,
    pub is_expired: bool,
    pub explanation: String,
}

/// Engine that computes deterministic feedback relevance decay and enforces persistence semantics.
pub struct ContextFeedbackRelevanceEngine;

impl ContextFeedbackRelevanceEngine {
    /// Classifies feedback into its corresponding persistence category based on action and source.
    pub fn classify_persistence_category(
        action: FeedbackAction,
        source: FeedbackSource,
    ) -> FeedbackPersistenceCategory {
        // Inferred or implicit feedback is always categorized as HistoricalInferred
        if !source.is_explicit_user() {
            return FeedbackPersistenceCategory::HistoricalInferred;
        }

        match action {
            // 1. Permanent Explicit Preferences
            FeedbackAction::ArchiveContext
            | FeedbackAction::ResumeContext
            | FeedbackAction::MarkImportant => FeedbackPersistenceCategory::PermanentPreference,

            // 2. Contextual Relationship Corrections
            FeedbackAction::ConfirmContext
            | FeedbackAction::RejectContext
            | FeedbackAction::RemoveFromContext
            | FeedbackAction::ConfirmTask
            | FeedbackAction::RejectTask
            | FeedbackAction::RemoveImportance => FeedbackPersistenceCategory::ContextualCorrection,

            // 3. Temporary Dismissals
            FeedbackAction::DismissSuggestion => FeedbackPersistenceCategory::TemporaryDismissal,
        }
    }

    /// Evaluates the relevance and decay of an explicit action against an evaluation timestamp.
    pub fn evaluate_action_relevance(
        action: FeedbackAction,
        source: FeedbackSource,
        created_at: DateTime<Utc>,
        eval_time: DateTime<Utc>,
        config: &FeedbackRelevanceConfig,
    ) -> FeedbackRelevanceAssessment {
        let category = Self::classify_persistence_category(action, source);
        let elapsed_secs = (eval_time - created_at).num_seconds().max(0);
        let elapsed_days = elapsed_secs as f64 / 86400.0;

        let base_weight = if action.is_positive() { 1.0 } else { 1.0 };

        let (decay_factor, is_active, is_expired, explanation) = match category {
            FeedbackPersistenceCategory::PermanentPreference => {
                // Permanent preferences NEVER decay or expire with time
                (
                    config.permanent_retention_weight,
                    true,
                    false,
                    format!(
                        "Permanent explicit preference '{}' maintains full weight ({:.2}) indefinitely ({} days elapsed)",
                        action.as_str(),
                        config.permanent_retention_weight,
                        elapsed_days as u64
                    ),
                )
            }
            FeedbackPersistenceCategory::ContextualCorrection => {
                // Contextual corrections maintain full strength for active window, then decay to anchor floor
                let factor = if elapsed_days <= config.contextual_correction_active_days {
                    1.0
                } else if elapsed_days <= config.contextual_correction_active_days + config.contextual_correction_decay_window_days {
                    let progress = (elapsed_days - config.contextual_correction_active_days)
                        / config.contextual_correction_decay_window_days;
                    1.0 - (1.0 - config.contextual_correction_floor) * progress as f32
                } else {
                    config.contextual_correction_floor
                };

                (
                    factor,
                    true,
                    false,
                    format!(
                        "Contextual correction '{}' active (decay factor: {:.2}, anchor floor: {:.2}, {:.1} days elapsed)",
                        action.as_str(),
                        factor,
                        config.contextual_correction_floor,
                        elapsed_days
                    ),
                )
            }
            FeedbackPersistenceCategory::TemporaryDismissal => {
                // Temporary dismissals decay linearly to 0 over the cooldown window
                if elapsed_days < config.temporary_dismissal_cooldown_days {
                    let factor = (1.0 - (elapsed_days / config.temporary_dismissal_cooldown_days) as f32).max(0.0);
                    (
                        factor,
                        true,
                        false,
                        format!(
                            "Temporary dismissal '{}' active in cooldown window (decay factor: {:.2}, {:.1}/{} days elapsed)",
                            action.as_str(),
                            factor,
                            elapsed_days,
                            config.temporary_dismissal_cooldown_days
                        ),
                    )
                } else {
                    (
                        0.0,
                        false,
                        true,
                        format!(
                            "Temporary dismissal '{}' expired after cooldown window of {} days ({:.1} days elapsed)",
                            action.as_str(),
                            config.temporary_dismissal_cooldown_days,
                            elapsed_days
                        ),
                    )
                }
            }
            FeedbackPersistenceCategory::HistoricalInferred => {
                // Exponential half-life decay
                let half_life = config.historical_inferred_half_life_days.max(1.0);
                let factor = (0.5_f64.powf(elapsed_days / half_life)) as f32;
                let is_active = factor >= 0.05;
                let is_expired = !is_active;

                (
                    factor,
                    is_active,
                    is_expired,
                    format!(
                        "Historical inferred feedback '{}' decayed with half-life {} days (decay factor: {:.3}, {:.1} days elapsed)",
                        action.as_str(),
                        half_life as u64,
                        factor,
                        elapsed_days
                    ),
                )
            }
        };

        let effective_weight = base_weight * decay_factor;

        FeedbackRelevanceAssessment {
            action,
            source,
            persistence_category: category,
            base_weight,
            effective_weight,
            elapsed_days,
            decay_factor,
            is_active,
            is_expired,
            explanation,
        }
    }

    /// Evaluates a ContextFeedbackEvent against the default relevance policy.
    pub fn evaluate_event_relevance(
        event: &ContextFeedbackEvent,
        eval_time: DateTime<Utc>,
        config: &FeedbackRelevanceConfig,
    ) -> FeedbackRelevanceAssessment {
        let created_at = event
            .timestamp_dt()
            .unwrap_or(eval_time);
        Self::evaluate_action_relevance(event.action, event.source, created_at, eval_time, config)
    }
}

// =========================================================================
// Development Feedback Inspector (Step 15)
// =========================================================================

/// Diagnostic inspection entry representing an auditable feedback event for development inspection.
/// Sensitive user text is scrubbed and bounded to prevent leaking raw contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackInspectorEntry {
    /// Unique event identifier
    pub event_id: String,
    /// Target entity identifier (item, task, workspace, memory, or suggestion)
    pub target_id: String,
    /// Type of target ("item", "task", "context", "memory", "suggestion")
    pub target_type: String,
    /// Privacy-safe label / sanitized title
    pub target_label: String,
    /// Associated workspace/context ID
    pub context_id: Option<String>,
    /// Associated workspace name if resolved
    pub context_name: Option<String>,
    /// Feedback scope (e.g. "item_context", "source_context", "global")
    pub scope: String,
    /// Specificity ranking of the scope (1 = narrowest/highest priority)
    pub scope_specificity: u8,
    /// Action executed (e.g. "remove_from_context", "confirm_context")
    pub action: String,
    /// User-facing display title for the action
    pub action_display: String,
    /// Feedback source / mechanism (e.g. "user_explicit_ui", "inbox_action")
    pub source: String,
    /// RFC3339 timestamp of the feedback action
    pub timestamp: String,
    /// Human-friendly relative time
    pub elapsed_human: String,
    /// Longevity and persistence category
    pub persistence_category: String,
    /// Effective state derived from conflict resolution (e.g. "Rejected", "Confirmed", "Ambiguous", "Neutral")
    pub effective_state: String,
    /// Current mathematical influence / decay factor [0.0, 1.0]
    pub current_influence: f32,
    /// Whether the feedback signal is currently mathematically active
    pub is_active: bool,
    /// Whether the feedback signal has expired based on policy
    pub is_expired: bool,
    /// Whether conflicting feedback signals were registered for this target
    pub has_conflict: bool,
    /// Total count of conflicting feedback signals
    pub conflicting_events_count: usize,
    /// Summary of conflicting feedback if any
    pub conflicting_summary: Option<String>,
    /// Diagnostic summary of related evidence signatures (scrubbed of sensitive text)
    pub related_evidence_summary: Vec<String>,
    /// Explanatory diagnostic reasoning
    pub explanation: String,
}

/// Diagnostic report inspecting all contextual feedback for a workspace/context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackInspectorContextReport {
    pub context_id: String,
    pub context_name: String,
    pub total_feedback_events: usize,
    pub active_feedback_events: usize,
    pub expired_feedback_events: usize,
    pub confirmations_count: usize,
    pub rejections_count: usize,
    pub conflicted_targets_count: usize,
    pub entries: Vec<FeedbackInspectorEntry>,
    pub generated_at: String,
}

/// Diagnostic report inspecting complete feedback history and effective state for a specific target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackInspectorTargetReport {
    pub target_id: String,
    pub target_label: String,
    pub context_id: Option<String>,
    pub context_name: Option<String>,
    pub effective_state: String,
    pub latest_action: Option<String>,
    pub current_influence: f32,
    pub history_entries: Vec<FeedbackInspectorEntry>,
    pub competing_conflicts: Vec<FeedbackInspectorEntry>,
    pub adaptation_verdict: Option<String>,
    pub diagnostic_explanation: String,
}

/// Development-only Inspector for auditing and diagnosing contextual feedback behavior,
/// conflict arbitration, and deterministic adaptation without exposing raw sensitive content.
pub struct ContextFeedbackInspector;

impl ContextFeedbackInspector {
    /// Inspects the complete feedback trajectory, effective state, and conflicts for a specific target entity.
    pub fn inspect_target(
        conn: &rusqlite::Connection,
        target_id: &str,
        context_id: Option<&str>,
    ) -> Result<FeedbackInspectorTargetReport, String> {
        let now = Utc::now();
        let relevance_config = FeedbackRelevanceConfig::default();

        // 1. Resolve target label safely
        let target_label = Self::resolve_safe_target_label(conn, target_id);

        // 2. Resolve context name if provided
        let context_name = context_id.and_then(|c_id| Self::resolve_context_name(conn, c_id));

        // 3. Query all context_signals related to this target
        let mut events: Vec<ContextFeedbackEvent> = Vec::new();
        let mut related_evidence_map: HashMap<String, Vec<String>> = HashMap::new();

        let query = if let Some(_c_id) = context_id {
            "SELECT id, workspace_id, signal_type, payload, created_at FROM context_signals
             WHERE (item_id = ?1 OR payload LIKE '%' || ?1 || '%') AND (workspace_id = ?2 OR workspace_id IS NULL)
             ORDER BY created_at ASC"
        } else {
            "SELECT id, workspace_id, signal_type, payload, created_at FROM context_signals
             WHERE item_id = ?1 OR payload LIKE '%' || ?1 || '%'
             ORDER BY created_at ASC"
        };

        let raw_rows: Vec<(String, Option<String>, String, String, String)> = if let Some(c_id) = context_id {
            let mut stmt = conn
                .prepare(query)
                .map_err(|e| format!("Failed to prepare inspector query: {}", e))?;
            let mapped = stmt
                .query_map(rusqlite::params![target_id, c_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(|e| format!("Query error: {}", e))?;
            mapped.filter_map(Result::ok).collect()
        } else {
            let mut stmt = conn
                .prepare(query)
                .map_err(|e| format!("Failed to prepare inspector query: {}", e))?;
            let mapped = stmt
                .query_map(rusqlite::params![target_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(|e| format!("Query error: {}", e))?;
            mapped.filter_map(Result::ok).collect()
        };

        for r in raw_rows {
            let (sig_id, ws_opt, sig_type, payload_str, created_str) = r;
            let mut related_evidence = Vec::new();

            let parsed_action = serde_json::from_str::<serde_json::Value>(&payload_str).ok();
            let action_opt = parsed_action.as_ref().and_then(|val| {
                if let Some(sig) = val.get("evidence_signature") {
                    if let Some(app) = sig.get("app_name").and_then(|v| v.as_str()) {
                        related_evidence.push(format!("Application: {}", app));
                    }
                    if let Some(domain) = sig.get("domain").and_then(|v| v.as_str()) {
                        related_evidence.push(format!("Domain: {}", domain));
                    }
                }
                val.get("action")
                    .and_then(|a| a.as_str())
                    .and_then(|s| FeedbackAction::from_str_lenient(s).ok())
            });

            let action = action_opt.unwrap_or_else(|| {
                if sig_type == "user_feedback"
                    || sig_type == "context_confirmed"
                    || sig_type == "membership_confirmed"
                    || sig_type == "task_confirmed"
                    || sig_type == "context_resumed"
                {
                    FeedbackAction::ConfirmContext
                } else if sig_type == "user_correction"
                    || sig_type == "membership_dismissed"
                    || sig_type == "user_removed_from_context"
                    || sig_type == "context_dismissed"
                {
                    FeedbackAction::RemoveFromContext
                } else if sig_type == "task_dismissed" || sig_type == "task_rejected" {
                    FeedbackAction::RejectTask
                } else if sig_type == "suggestion_dismissed" {
                    FeedbackAction::DismissSuggestion
                } else if sig_type == "context_archived" {
                    FeedbackAction::ArchiveContext
                } else {
                    FeedbackAction::RejectContext
                }
            });

            let target_ref = FeedbackTargetRef::infer_from_id(target_id);
            let scope = FeedbackScope::infer_narrowest_scope(action, &target_ref, ws_opt.as_deref());

            let mut ev = ContextFeedbackEvent::new(
                action,
                target_ref,
                ws_opt,
                FeedbackSource::UserExplicitUi,
                scope,
                None,
            )
            .with_timestamp(created_str);
            ev.id = sig_id.clone();

            related_evidence_map.insert(sig_id, related_evidence);
            events.push(ev);
        }

        // 4. Resolve effective state using conflict resolver
        let target_ctx = context_id.unwrap_or("");
        let conflict_resolution = FeedbackConflictResolver::resolve_events(&events, target_id, target_ctx);

        let effective_state_str = match &conflict_resolution.effective_state {
            EffectiveFeedbackState::Confirmed { .. } => "Confirmed",
            EffectiveFeedbackState::Rejected { .. } => "Rejected",
            EffectiveFeedbackState::Ambiguous { .. } => "Ambiguous",
            EffectiveFeedbackState::Neutral => "Neutral",
        }
        .to_string();

        // 5. Build inspector entries
        let mut history_entries = Vec::new();
        let mut competing_conflicts = Vec::new();

        for ev in &events {
            let relevance = ContextFeedbackRelevanceEngine::evaluate_event_relevance(ev, now, &relevance_config);
            let elapsed_human = Self::format_relative_time(ev.timestamp_dt().unwrap_or(now), now);
            let related = related_evidence_map.get(&ev.id).cloned().unwrap_or_default();

            let is_conflicted = (conflict_resolution.confirmations_count > 0
                && conflict_resolution.rejections_count > 0)
                || conflict_resolution.is_ambiguous;

            let entry = FeedbackInspectorEntry {
                event_id: ev.id.clone(),
                target_id: target_id.to_string(),
                target_type: ev.target_ref.entity_type_str().to_string(),
                target_label: target_label.clone(),
                context_id: ev.context_ref.clone(),
                context_name: ev.context_ref.as_deref().and_then(|c| Self::resolve_context_name(conn, c)),
                scope: ev.scope.scope_str().to_string(),
                scope_specificity: ev.scope.specificity_order(),
                action: ev.action.as_str().to_string(),
                action_display: ev.action.display_name().to_string(),
                source: ev.source.as_str().to_string(),
                timestamp: ev.timestamp.clone(),
                elapsed_human,
                persistence_category: relevance.persistence_category.display_name().to_string(),
                effective_state: effective_state_str.clone(),
                current_influence: relevance.effective_weight,
                is_active: relevance.is_active,
                is_expired: relevance.is_expired,
                has_conflict: is_conflicted,
                conflicting_events_count: conflict_resolution.total_history_count.saturating_sub(1),
                conflicting_summary: if is_conflicted {
                    Some(format!(
                        "{} confirmations vs {} rejections recorded",
                        conflict_resolution.confirmations_count, conflict_resolution.rejections_count
                    ))
                } else {
                    None
                },
                related_evidence_summary: related,
                explanation: relevance.explanation,
            };

            if is_conflicted && ev.action.is_positive() != (effective_state_str == "Confirmed") {
                competing_conflicts.push(entry.clone());
            }
            history_entries.push(entry);
        }

        let latest_action = conflict_resolution.latest_action.map(|a| a.display_name().to_string());
        let current_influence = history_entries.last().map(|e| e.current_influence).unwrap_or(1.0);

        let diagnostic_explanation = format!(
            "Target '{}' in context '{}': Effective State = {}. {} total feedback event(s) recorded. Latest action = {}.",
            target_label,
            context_name.as_deref().unwrap_or("Global"),
            effective_state_str,
            history_entries.len(),
            latest_action.as_deref().unwrap_or("None")
        );

        Ok(FeedbackInspectorTargetReport {
            target_id: target_id.to_string(),
            target_label,
            context_id: context_id.map(|s| s.to_string()),
            context_name,
            effective_state: effective_state_str,
            latest_action,
            current_influence,
            history_entries,
            competing_conflicts,
            adaptation_verdict: None,
            diagnostic_explanation,
        })
    }

    /// Inspects all feedback events associated with a workspace context.
    pub fn inspect_context(
        conn: &rusqlite::Connection,
        context_id: &str,
    ) -> Result<FeedbackInspectorContextReport, String> {
        let now = Utc::now();
        let relevance_config = FeedbackRelevanceConfig::default();
        let context_name = Self::resolve_context_name(conn, context_id).unwrap_or_else(|| context_id.to_string());

        let mut entries = Vec::new();
        let mut confirmations_count = 0;
        let mut rejections_count = 0;
        let mut active_count = 0;
        let mut expired_count = 0;
        let mut conflicted_targets: HashSet<String> = HashSet::new();
        let mut target_actions: HashMap<String, (usize, usize)> = HashMap::new();

        let mut stmt = conn
            .prepare(
                "SELECT id, item_id, signal_type, payload, created_at FROM context_signals
                 WHERE workspace_id = ?1
                 ORDER BY created_at DESC",
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let rows = stmt
            .query_map(rusqlite::params![context_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| format!("Query error: {}", e))?;

        for r in rows.flatten() {
            let (sig_id, item_opt, sig_type, payload_str, created_str) = r;
            let target_id = item_opt.unwrap_or_else(|| "unknown_target".to_string());
            let target_label = Self::resolve_safe_target_label(conn, &target_id);

            let parsed_action = serde_json::from_str::<serde_json::Value>(&payload_str).ok();
            let action_opt = parsed_action.as_ref().and_then(|val| {
                val.get("action")
                    .and_then(|a| a.as_str())
                    .and_then(|s| FeedbackAction::from_str_lenient(s).ok())
            });

            let action = action_opt.unwrap_or_else(|| {
                if sig_type == "user_feedback" || sig_type == "context_confirmed" {
                    FeedbackAction::ConfirmContext
                } else if sig_type == "user_correction" || sig_type == "membership_dismissed" {
                    FeedbackAction::RemoveFromContext
                } else {
                    FeedbackAction::RejectContext
                }
            });

            let entry_stats = target_actions.entry(target_id.clone()).or_insert((0, 0));
            if action.is_positive() {
                confirmations_count += 1;
                entry_stats.0 += 1;
            } else if action.is_negative() {
                rejections_count += 1;
                entry_stats.1 += 1;
            }
            if entry_stats.0 > 0 && entry_stats.1 > 0 {
                conflicted_targets.insert(target_id.clone());
            }

            let target_ref = FeedbackTargetRef::infer_from_id(&target_id);
            let scope = FeedbackScope::infer_narrowest_scope(action, &target_ref, Some(context_id));

            let ev = ContextFeedbackEvent::new(
                action,
                target_ref,
                Some(context_id.to_string()),
                FeedbackSource::UserExplicitUi,
                scope.clone(),
                None,
            )
            .with_timestamp(created_str.clone());

            let relevance = ContextFeedbackRelevanceEngine::evaluate_event_relevance(&ev, now, &relevance_config);
            let elapsed_human = Self::format_relative_time(ev.timestamp_dt().unwrap_or(now), now);

            if relevance.is_active {
                active_count += 1;
            } else {
                expired_count += 1;
            }

            entries.push(FeedbackInspectorEntry {
                event_id: sig_id,
                target_id: target_id.clone(),
                target_type: ev.target_ref.entity_type_str().to_string(),
                target_label,
                context_id: Some(context_id.to_string()),
                context_name: Some(context_name.clone()),
                scope: scope.scope_str().to_string(),
                scope_specificity: scope.specificity_order(),
                action: action.as_str().to_string(),
                action_display: action.display_name().to_string(),
                source: "user_explicit_ui".to_string(),
                timestamp: created_str,
                elapsed_human,
                persistence_category: relevance.persistence_category.display_name().to_string(),
                effective_state: if action.is_positive() { "Confirmed".to_string() } else { "Rejected".to_string() },
                current_influence: relevance.effective_weight,
                is_active: relevance.is_active,
                is_expired: relevance.is_expired,
                has_conflict: false,
                conflicting_events_count: 0,
                conflicting_summary: None,
                related_evidence_summary: Vec::new(),
                explanation: relevance.explanation,
            });
        }

        Ok(FeedbackInspectorContextReport {
            context_id: context_id.to_string(),
            context_name,
            total_feedback_events: entries.len(),
            active_feedback_events: active_count,
            expired_feedback_events: expired_count,
            confirmations_count,
            rejections_count,
            conflicted_targets_count: conflicted_targets.len(),
            entries,
            generated_at: now.to_rfc3339(),
        })
    }

    /// Formats a single feedback inspector entry as a structured debug card.
    pub fn format_entry_card(entry: &FeedbackInspectorEntry) -> String {
        format!(
            "Target: {}\nContext: {}\n\nUser feedback:\n{}\n\nScope:\n{}\n\nEffective:\n{}\nCurrent Influence: {:.2} (Active: {})\nExplanation: {}\n",
            entry.target_label,
            entry.context_name.as_deref().unwrap_or("Global"),
            entry.action_display,
            entry.scope,
            entry.effective_state,
            entry.current_influence,
            entry.is_active,
            entry.explanation
        )
    }

    // --- Private Helper Methods ---

    fn resolve_safe_target_label(conn: &rusqlite::Connection, target_id: &str) -> String {
        // Try items table
        if let Ok(title) = conn.query_row(
            "SELECT title, content FROM items WHERE id = ?1",
            rusqlite::params![target_id],
            |r| {
                let t: Option<String> = r.get(0)?;
                let c: String = r.get(1)?;
                Ok(t.unwrap_or(c))
            },
        ) {
            let scrubbed = crate::context_engine::reasoning::privacy::scrub_private_data(&title);
            if scrubbed.len() > 60 {
                return format!("{}...", &scrubbed[..57]);
            }
            return scrubbed;
        }

        // Try task_candidates table
        if let Ok(title) = conn.query_row(
            "SELECT title FROM task_candidates WHERE id = ?1",
            rusqlite::params![target_id],
            |r| r.get::<_, String>(0),
        ) {
            let scrubbed = crate::context_engine::reasoning::privacy::scrub_private_data(&title);
            return scrubbed;
        }

        // Try workspaces table
        if let Ok(name) = conn.query_row(
            "SELECT name FROM workspaces WHERE id = ?1",
            rusqlite::params![target_id],
            |r| r.get::<_, String>(0),
        ) {
            return format!("Workspace '{}'", name);
        }

        format!("Target #{}", &target_id[..target_id.len().min(8)])
    }

    fn resolve_context_name(conn: &rusqlite::Connection, context_id: &str) -> Option<String> {
        conn.query_row(
            "SELECT name FROM workspaces WHERE id = ?1",
            rusqlite::params![context_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    fn format_relative_time(created_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
        let diff = (now - created_at).num_seconds().max(0);
        let mins = diff / 60;
        let hours = mins / 60;
        let days = hours / 24;

        if days > 0 {
            format!("{} day(s) ago", days)
        } else if hours > 0 {
            format!("{} hour(s) ago", hours)
        } else if mins > 0 {
            format!("{} minute(s) ago", mins)
        } else {
            "just now".to_string()
        }
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
        assert_eq!(ev1.scope, FeedbackScope::item_context("item-123", "ws-payx"));
        assert!(ev1.validate().is_ok());

        let ev2 = ContextFeedbackEvent::remove_from_context(
            "item-456",
            "ws-payx",
            FeedbackSource::WorksetTray,
            Some("Accidental capture".to_string()),
        );
        assert_eq!(ev2.action, FeedbackAction::RemoveFromContext);
        assert_eq!(ev2.reason.as_deref(), Some("Accidental capture"));
        assert_eq!(ev2.scope, FeedbackScope::item_context("item-456", "ws-payx"));
        assert!(ev2.validate().is_ok());

        let ev3 = ContextFeedbackEvent::mark_important(
            FeedbackTargetRef::Memory("mem-789".to_string()),
            Some("ws-payx".to_string()),
            FeedbackSource::MemoryInspectionPanel,
        );
        assert_eq!(ev3.action, FeedbackAction::MarkImportant);
        assert_eq!(ev3.target_ref, FeedbackTargetRef::Memory("mem-789".to_string()));
        assert_eq!(ev3.scope, FeedbackScope::item_context("mem-789", "ws-payx"));
        assert!(ev3.validate().is_ok());

        let ev4 = ContextFeedbackEvent::confirm_task("task-001", "ws-payx", FeedbackSource::UserExplicitUi);
        assert_eq!(ev4.action, FeedbackAction::ConfirmTask);
        assert_eq!(ev4.target_ref, FeedbackTargetRef::Task("task-001".to_string()));
        assert_eq!(ev4.scope, FeedbackScope::task_context("task-001", "ws-payx"));
        assert!(ev4.validate().is_ok());

        let ev5 = ContextFeedbackEvent::reject_task(
            "task-002",
            Some("ws-payx".to_string()),
            FeedbackSource::InboxAction,
            Some("Not my task".to_string()),
        );
        assert_eq!(ev5.action, FeedbackAction::RejectTask);
        assert_eq!(ev5.scope, FeedbackScope::task_context("task-002", "ws-payx"));
        assert!(ev5.validate().is_ok());

        let ev6 = ContextFeedbackEvent::dismiss_suggestion(
            "sugg-001",
            Some("ws-payx".to_string()),
            FeedbackSource::InboxAction,
            None,
        );
        assert_eq!(ev6.action, FeedbackAction::DismissSuggestion);
        assert_eq!(ev6.scope, FeedbackScope::item_context("sugg-001", "ws-payx"));
        assert!(ev6.validate().is_ok());

        let ev7 = ContextFeedbackEvent::resume_context("ws-payx", FeedbackSource::UserExplicitUi);
        assert_eq!(ev7.action, FeedbackAction::ResumeContext);
        assert_eq!(ev7.target_ref, FeedbackTargetRef::Context("ws-payx".to_string()));
        assert_eq!(ev7.scope, FeedbackScope::context("ws-payx"));
        assert!(ev7.validate().is_ok());

        let ev8 = ContextFeedbackEvent::archive_context("ws-old", FeedbackSource::ContextMenu, None);
        assert_eq!(ev8.action, FeedbackAction::ArchiveContext);
        assert_eq!(ev8.scope, FeedbackScope::context("ws-old"));
        assert!(ev8.validate().is_ok());

        let ev9 = ContextFeedbackEvent::relationship_feedback(
            FeedbackAction::ConfirmContext,
            "repo:github.com/org/repo-x",
            "ws-payx",
            "git_repository",
            FeedbackSource::UserExplicitUi,
            Some("Linked repository".to_string()),
        );
        assert_eq!(ev9.action, FeedbackAction::ConfirmContext);
        assert_eq!(
            ev9.scope,
            FeedbackScope::relationship("repo:github.com/org/repo-x", "ws-payx", "git_repository")
        );
        assert!(ev9.validate().is_ok());
    }

    #[test]
    fn test_narrowest_scope_inference() {
        // 1. Removing an item from PayX -> ItemContext (NOT Global!)
        let scope_item = FeedbackScope::infer_narrowest_scope(
            FeedbackAction::RemoveFromContext,
            &FeedbackTargetRef::Item("doc-101".to_string()),
            Some("PayX"),
        );
        assert_eq!(scope_item, FeedbackScope::item_context("doc-101", "PayX"));
        assert_eq!(scope_item.granularity_rank(), 1); // Narrowest rank!

        // 2. Rejecting task from PayX -> TaskContext
        let scope_task = FeedbackScope::infer_narrowest_scope(
            FeedbackAction::RejectTask,
            &FeedbackTargetRef::Task("task-202".to_string()),
            Some("PayX"),
        );
        assert_eq!(scope_task, FeedbackScope::task_context("task-202", "PayX"));
        assert_eq!(scope_task.granularity_rank(), 3);

        // 3. Resuming PayX -> Context
        let scope_ctx = FeedbackScope::infer_narrowest_scope(
            FeedbackAction::ResumeContext,
            &FeedbackTargetRef::Context("PayX".to_string()),
            Some("PayX"),
        );
        assert_eq!(scope_ctx, FeedbackScope::context("PayX"));
        assert_eq!(scope_ctx.granularity_rank(), 5);

        // 4. Global scope is only used when completely unassociated with any context
        let scope_global = FeedbackScope::infer_narrowest_scope(
            FeedbackAction::MarkImportant,
            &FeedbackTargetRef::Item("item-floating".to_string()),
            None,
        );
        assert_eq!(scope_global, FeedbackScope::global());
        assert_eq!(scope_global.granularity_rank(), 6);
    }

    #[test]
    fn test_scope_isolation_between_workspaces() {
        let scope_payx = FeedbackScope::item_context("item-123", "PayX");
        let scope_atlas = FeedbackScope::item_context("item-456", "Atlas");

        // PayX scope is strictly isolated from Atlas
        assert!(scope_payx.is_isolated_from("Atlas"));
        assert!(scope_payx.is_isolated_from("Research"));
        assert!(!scope_payx.is_isolated_from("PayX"));

        // Atlas scope is strictly isolated from PayX
        assert!(scope_atlas.is_isolated_from("PayX"));
        assert!(!scope_atlas.is_isolated_from("Atlas"));

        // Global scope is NEVER isolated from any workspace
        let scope_global = FeedbackScope::global();
        assert!(!scope_global.is_isolated_from("PayX"));
        assert!(!scope_global.is_isolated_from("Atlas"));
    }

    #[test]
    fn test_scope_matching_and_specificity_rules() {
        // Scenario A: User rejects GitHub repository X from PayX
        let repo_x_scope = FeedbackScope::relationship(
            "repo:github.com/org/repo-x",
            "context:PayX",
            "git_repository",
        );

        // Repo X in PayX matches!
        assert!(repo_x_scope.applies_to(
            Some("repo:github.com/org/repo-x"),
            Some("github.com/org/repo-x"),
            None,
            "PayX"
        ));

        // GitHub repository Y in PayX DOES NOT MATCH! (Does not reject all GitHub repositories from PayX)
        assert!(!repo_x_scope.applies_to(
            Some("repo:github.com/org/repo-y"),
            Some("github.com/org/repo-y"),
            None,
            "PayX"
        ));

        // GitHub repository X in Atlas DOES NOT MATCH! (Does not affect Atlas)
        assert!(!repo_x_scope.applies_to(
            Some("repo:github.com/org/repo-x"),
            Some("github.com/org/repo-x"),
            None,
            "Atlas"
        ));

        // Scenario B: User says "Remove item-99 from PayX" -> ItemContext
        let item_scope = FeedbackScope::item_context("item-99", "PayX");

        // Exactly item-99 in PayX matches
        assert!(item_scope.applies_to(Some("item-99"), None, None, "PayX"));

        // Other items in PayX do not match
        assert!(!item_scope.applies_to(Some("item-100"), None, None, "PayX"));

        // Same item in Atlas does not match
        assert!(!item_scope.applies_to(Some("item-99"), None, None, "Atlas"));
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
        assert_eq!(deserialized.scope, FeedbackScope::item_context("item-999", "ws-payx"));
    }

    #[test]
    fn test_audit_summary_format() {
        let ev = ContextFeedbackEvent::confirm_context("item-abc", "ws-123", FeedbackSource::UserExplicitUi);
        let summary = ev.audit_summary();
        assert!(summary.contains("Action='confirm_context'"));
        assert!(summary.contains("Target='item:item-abc'"));
        assert!(summary.contains("Context='ws-123'"));
        assert!(summary.contains("Source='user_explicit_ui'"));
        assert!(summary.contains("Scope='item_context'"));
    }

    #[test]
    fn test_invalid_event_validation() {
        let mut invalid_ev = ContextFeedbackEvent::confirm_context("item-abc", "ws-123", FeedbackSource::UserExplicitUi);
        invalid_ev.id = "   ".to_string();
        assert!(invalid_ev.validate().is_err());

        let mut invalid_ws = ContextFeedbackEvent::confirm_context("item-abc", "ws-123", FeedbackSource::UserExplicitUi);
        invalid_ws.scope = FeedbackScope::item_context("item-abc", "");
        assert!(invalid_ws.validate().is_err());

        let mut invalid_rel = ContextFeedbackEvent::confirm_context("item-abc", "ws-123", FeedbackSource::UserExplicitUi);
        invalid_rel.scope = FeedbackScope::relationship("", "ws-123", "kind");
        assert!(invalid_rel.validate().is_err());
    }

    fn setup_test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_context_feedback_service_apply_all_actions() {
        let conn = setup_test_db();

        // 1. Create test workspace and items
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/payx/api-service",
            Some("PayX API Service"),
            "{\"domain\":\"github.com\",\"app_name\":\"Google Chrome\"}",
        )
        .unwrap();

        // ---------------------------------------------------------------------
        // Action 1: ConfirmContext
        // ---------------------------------------------------------------------
        let confirm_ev = ContextFeedbackEvent::confirm_context(&item.id, &ws_payx.id, FeedbackSource::UserExplicitUi);
        let res_confirm = ContextFeedbackService::apply_feedback(&conn, &confirm_ev).unwrap();
        assert!(res_confirm.success);
        assert!(res_confirm.applied_systems.contains(&"context_memberships".to_string()));
        assert!(res_confirm.applied_systems.contains(&"items".to_string()));

        // Verify membership is confirmed
        let (mem_type, conf): (String, f64) = conn
            .query_row(
                "SELECT membership_type, confidence FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
                rusqlite::params![item.id, ws_payx.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(mem_type, "confirmed");
        assert_eq!(conf, 1.0);

        // Verify item.workspace_id is set
        let updated_item = crate::db::operations::get_item(&conn, &item.id).unwrap();
        assert_eq!(updated_item.workspace_id.as_deref(), Some(ws_payx.id.as_str()));

        // ---------------------------------------------------------------------
        // Action 2: RemoveFromContext (Context Correction)
        // ---------------------------------------------------------------------
        let remove_ev = ContextFeedbackEvent::remove_from_context(
            &item.id,
            &ws_payx.id,
            FeedbackSource::WorksetTray,
            Some("Wrong workspace".to_string()),
        );
        let res_remove = ContextFeedbackService::apply_feedback(&conn, &remove_ev).unwrap();
        assert!(res_remove.success);
        assert!(res_remove.applied_systems.contains(&"context_correction_service".to_string()));

        // Verify membership demoted to dismissed (confidence 0.0)
        let (mem_after_type, mem_after_conf): (String, f64) = conn
            .query_row(
                "SELECT membership_type, confidence FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
                rusqlite::params![item.id, ws_payx.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(mem_after_type, "dismissed");
        assert_eq!(mem_after_conf, 0.0);

        // Verify item is unassigned but NEVER deleted (Zero Data Loss invariant)
        let unassigned_item = crate::db::operations::get_item(&conn, &item.id).unwrap();
        assert!(unassigned_item.workspace_id.is_none());
        assert_eq!(unassigned_item.id, item.id);

        // ---------------------------------------------------------------------
        // Action 3: MarkImportant & RemoveImportance
        // ---------------------------------------------------------------------
        let mark_ev = ContextFeedbackEvent::mark_important(
            FeedbackTargetRef::Item(item.id.clone()),
            Some(ws_payx.id.clone()),
            FeedbackSource::MemoryInspectionPanel,
        );
        let res_mark = ContextFeedbackService::apply_feedback(&conn, &mark_ev).unwrap();
        assert!(res_mark.success);
        assert!(res_mark.applied_systems.contains(&"important_memory_service".to_string()));

        let unmark_ev = ContextFeedbackEvent::remove_importance(
            FeedbackTargetRef::Item(item.id.clone()),
            Some(ws_payx.id.clone()),
            FeedbackSource::MemoryInspectionPanel,
        );
        let res_unmark = ContextFeedbackService::apply_feedback(&conn, &unmark_ev).unwrap();
        assert!(res_unmark.success);

        // ---------------------------------------------------------------------
        // Action 4: ConfirmTask & RejectTask
        // ---------------------------------------------------------------------
        let task_cand = crate::db::models::TaskCandidate::new(
            Some(ws_payx.id.clone()),
            "Implement Stripe Webhooks",
            0.70,
        );
        let created_task = crate::db::operations::create_task_candidate(&conn, &task_cand).unwrap();

        // Confirm task -> Confirmed (active)
        let confirm_task_ev = ContextFeedbackEvent::confirm_task(&created_task.id, &ws_payx.id, FeedbackSource::UserExplicitUi);
        let res_confirm_task = ContextFeedbackService::apply_feedback(&conn, &confirm_task_ev).unwrap();
        assert!(res_confirm_task.success);
        let active_task = crate::db::operations::get_task_candidate(&conn, &created_task.id).unwrap();
        assert!(active_task.status.is_active());
        assert_eq!(active_task.status, crate::db::models::TaskCandidateStatus::Confirmed);

        // Reject task -> Dismissed
        let reject_task_ev = ContextFeedbackEvent::reject_task(&created_task.id, Some(ws_payx.id.clone()), FeedbackSource::InboxAction, None);
        let res_reject_task = ContextFeedbackService::apply_feedback(&conn, &reject_task_ev).unwrap();
        assert!(res_reject_task.success);
        let dismissed_task = crate::db::operations::get_task_candidate(&conn, &created_task.id).unwrap();
        assert_eq!(dismissed_task.status, crate::db::models::TaskCandidateStatus::Dismissed);

        // ---------------------------------------------------------------------
        // Action 5: DismissSuggestion
        // ---------------------------------------------------------------------
        let sugg_item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://random-docs.com",
            Some("Random Docs"),
            "{}",
        )
        .unwrap();

        let dismiss_sugg_ev = ContextFeedbackEvent::dismiss_suggestion(
            &sugg_item.id,
            Some(ws_payx.id.clone()),
            FeedbackSource::InboxAction,
            Some("Not needed".to_string()),
        );
        let res_dismiss = ContextFeedbackService::apply_feedback(&conn, &dismiss_sugg_ev).unwrap();
        assert!(res_dismiss.success);
        assert!(res_dismiss.applied_systems.contains(&"inbox_state_machine".to_string()));

        let inbox_state = crate::context_engine::inbox_state::InboxStateMachine::get_state(&conn, &sugg_item.id).unwrap();
        assert_eq!(inbox_state.unwrap().state, crate::db::models::InboxState::Dismissed);

        // ---------------------------------------------------------------------
        // Action 6: ResumeContext & ArchiveContext
        // ---------------------------------------------------------------------
        // Resume context -> active
        let resume_ev = ContextFeedbackEvent::resume_context(&ws_payx.id, FeedbackSource::UserExplicitUi);
        let res_resume = ContextFeedbackService::apply_feedback(&conn, &resume_ev).unwrap();
        assert!(res_resume.success);
        let ws_resumed = crate::db::operations::get_workspace(&conn, &ws_payx.id).unwrap();
        assert_eq!(ws_resumed.lifecycle_state, "active");

        // Archive context -> archived
        let archive_ev = ContextFeedbackEvent::archive_context(&ws_payx.id, FeedbackSource::ContextMenu, None);
        let res_archive = ContextFeedbackService::apply_feedback(&conn, &archive_ev).unwrap();
        assert!(res_archive.success);
        let ws_archived = crate::db::operations::get_workspace(&conn, &ws_payx.id).unwrap();
        assert_eq!(ws_archived.lifecycle_state, "archived");
    }

    #[test]
    fn test_negative_feedback_repeated_rejection() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws_payx.id),
            None,
            "url",
            "https://medium.com/tech/random-article",
            Some("Random Tech Article"),
            "{\"domain\":\"medium.com\"}",
        )
        .unwrap();

        // 1st rejection
        let ev1 = ContextFeedbackEvent::remove_from_context(&item.id, &ws_payx.id, FeedbackSource::WorksetTray, None);
        ContextFeedbackService::apply_feedback(&conn, &ev1).unwrap();

        let assess1 = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert!(assess1.is_discouraged);
        assert!(!assess1.is_overridden);
        assert_eq!(assess1.rejection_count, 1);
        assert_eq!(assess1.base_penalty, 0.40);
        assert!(assess1.verdict.is_discouraged());

        // 2nd rejection
        let ev2 = ContextFeedbackEvent::reject_context(&item.id, &ws_payx.id, FeedbackSource::InboxAction, Some("Still not relevant".to_string()));
        ContextFeedbackService::apply_feedback(&conn, &ev2).unwrap();

        let assess2 = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert_eq!(assess2.rejection_count, 2);
        assert_eq!(assess2.base_penalty, 0.65);
        assert!(assess2.effective_penalty >= 0.60);

        // 3rd rejection
        let ev3 = ContextFeedbackEvent::reject_context(&item.id, &ws_payx.id, FeedbackSource::InboxAction, Some("Definitely wrong context".to_string()));
        ContextFeedbackService::apply_feedback(&conn, &ev3).unwrap();

        let assess3 = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert_eq!(assess3.rejection_count, 3);
        assert_eq!(assess3.base_penalty, 0.80);
    }

    #[test]
    fn test_negative_feedback_later_confirmation() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://medium.com/tech/random-article",
            Some("Random Tech Article"),
            "{\"domain\":\"medium.com\"}",
        )
        .unwrap();

        // 1. User rejects item from PayX at T0
        let t0 = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let rej_ev = ContextFeedbackEvent::reject_context(&item.id, &ws_payx.id, FeedbackSource::InboxAction, None)
            .with_timestamp(t0);
        ContextFeedbackService::apply_feedback(&conn, &rej_ev).unwrap();

        let assess_before = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert!(assess_before.is_discouraged);
        assert_eq!(assess_before.rejection_count, 1);

        // 2. User explicitly confirms item into PayX at T1 > T0
        let t1 = Utc::now().to_rfc3339();
        let conf_ev = ContextFeedbackEvent::confirm_context(&item.id, &ws_payx.id, FeedbackSource::UserExplicitUi)
            .with_timestamp(t1);
        ContextFeedbackService::apply_feedback(&conn, &conf_ev).unwrap();

        // 3. Negative feedback is overridden by explicit later confirmation
        let assess_after = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert!(!assess_after.is_discouraged);
        assert!(assess_after.is_overridden);
        assert_eq!(assess_after.effective_penalty, 0.0);
        assert!(assess_after.verdict.is_allowed());
        assert!(matches!(assess_after.verdict, NegativeFeedbackVerdict::OverriddenByExplicitConfirmation { .. }));
    }

    #[test]
    fn test_negative_feedback_different_context_isolation() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let ws_personal = crate::db::operations::create_workspace(&conn, "Personal").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws_payx.id),
            None,
            "url",
            "https://medium.com/tech/random-article",
            Some("Random Tech Article"),
            "{\"domain\":\"medium.com\"}",
        )
        .unwrap();

        // Reject from PayX
        let rej_ev = ContextFeedbackEvent::remove_from_context(&item.id, &ws_payx.id, FeedbackSource::WorksetTray, None);
        ContextFeedbackService::apply_feedback(&conn, &rej_ev).unwrap();

        // PayX is discouraged
        let assess_payx = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert!(assess_payx.is_discouraged);
        assert_eq!(assess_payx.rejection_count, 1);

        // Personal is completely unaffected (Scope Isolation)
        let assess_personal = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_personal.id, 0.50, false, false);
        assert!(!assess_personal.is_discouraged);
        assert_eq!(assess_personal.rejection_count, 0);
        assert_eq!(assess_personal.effective_penalty, 0.0);
        assert_eq!(assess_personal.verdict, NegativeFeedbackVerdict::Allowed);
    }

    #[test]
    fn test_negative_feedback_conflicting_feedback() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/payx/core",
            Some("PayX Core Repo"),
            "{\"domain\":\"github.com\"}",
        )
        .unwrap();

        let now = Utc::now();
        let t0 = (now - chrono::Duration::hours(3)).to_rfc3339();
        let t1 = (now - chrono::Duration::hours(2)).to_rfc3339();
        let t2 = (now - chrono::Duration::hours(1)).to_rfc3339();

        // 1. Confirm at T0
        let ev_t0 = ContextFeedbackEvent::confirm_context(&item.id, &ws_payx.id, FeedbackSource::UserExplicitUi)
            .with_timestamp(t0);
        ContextFeedbackService::apply_feedback(&conn, &ev_t0).unwrap();

        // 2. Reject at T1 > T0 -> Rejection is newest, negative feedback active
        let ev_t1 = ContextFeedbackEvent::reject_context(&item.id, &ws_payx.id, FeedbackSource::InboxAction, None)
            .with_timestamp(t1);
        ContextFeedbackService::apply_feedback(&conn, &ev_t1).unwrap();

        let assess_t1 = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert!(assess_t1.is_discouraged);
        assert!(matches!(assess_t1.verdict, NegativeFeedbackVerdict::Discouraged { .. }));

        // 3. Confirm at T2 > T1 -> Affirmative confirmation is newest, overrides prior rejection
        let ev_t2 = ContextFeedbackEvent::confirm_context(&item.id, &ws_payx.id, FeedbackSource::UserExplicitUi)
            .with_timestamp(t2);
        ContextFeedbackService::apply_feedback(&conn, &ev_t2).unwrap();

        let assess_t2 = ContextNegativeFeedbackEvaluator::evaluate(&conn, &item.id, &ws_payx.id, 0.50, false, false);
        assert!(!assess_t2.is_discouraged);
        assert!(assess_t2.is_overridden);
        assert_eq!(assess_t2.effective_penalty, 0.0);
    }

    #[test]
    fn test_negative_feedback_stale_decay() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://docs.rs/serde",
            Some("Serde Docs"),
            "{\"domain\":\"docs.rs\"}",
        )
        .unwrap();

        // Record rejection 90 days ago
        let now = Utc::now();
        let t_90_days_ago = (now - chrono::Duration::days(90)).to_rfc3339();

        let rej_ev = ContextFeedbackEvent::reject_context(&item.id, &ws_payx.id, FeedbackSource::InboxAction, None)
            .with_timestamp(t_90_days_ago);
        ContextFeedbackService::apply_feedback(&conn, &rej_ev).unwrap();

        // Evaluate at current time (90 days elapsed)
        let assess = ContextNegativeFeedbackEvaluator::evaluate_with_time(
            &conn,
            &item.id,
            &ws_payx.id,
            0.50,
            false,
            false,
            now,
        );

        assert!(assess.is_stale);
        assert_eq!(assess.base_penalty, 0.40);
        // After 90 days (3 half-lives of 30 days = 1/8 of base penalty), penalty should decay to ~0.05
        assert!(assess.effective_penalty < 0.10);
        assert!(assess.effective_penalty > 0.0);

        // If strong corroborating evidence appears (conf: 0.90 + strong anchor), it can legitimately override
        let assess_override = ContextNegativeFeedbackEvaluator::evaluate_with_time(
            &conn,
            &item.id,
            &ws_payx.id,
            0.90,
            true,
            false,
            now,
        );
        assert!(assess_override.is_overridden);
        assert_eq!(assess_override.effective_penalty, 0.0);
        assert!(matches!(assess_override.verdict, NegativeFeedbackVerdict::OverriddenByStrongEvidence { .. }));
    }

    #[test]
    fn test_positive_feedback_strengthens_confirmed_item() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/payx/core",
            Some("PayX Core Repository"),
            "{\"domain\":\"github.com\",\"repo\":\"payx/core\"}",
        )
        .unwrap();

        // 1. Initially unconfirmed
        let initial_assess = ContextPositiveFeedbackEngine::evaluate_confirmed_relationship(&conn, &item.id, &ws_payx.id);
        assert!(!initial_assess.is_confirmed);
        assert_eq!(initial_assess.relationship_strength, 0.0);

        // 2. User confirms item into PayX
        let conf_ev = ContextFeedbackEvent::confirm_context(&item.id, &ws_payx.id, FeedbackSource::UserExplicitUi);
        let res = ContextFeedbackService::apply_feedback(&conn, &conf_ev).unwrap();
        assert!(res.success);

        // 3. Positive relationship is strongly supported (confidence: 1.0, relationship_strength: 1.0)
        let assess = ContextPositiveFeedbackEngine::evaluate_confirmed_relationship(&conn, &item.id, &ws_payx.id);
        assert!(assess.is_confirmed);
        assert_eq!(assess.confidence, 1.0);
        assert_eq!(assess.relationship_strength, 1.0);
        assert!(assess.is_localized);
        assert!(assess.confirmed_at.is_some());
    }

    #[test]
    fn test_positive_feedback_preserves_evidence_localization() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        // Target repository item
        let repo_item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/payx/core",
            Some("PayX Core Repo"),
            "{\"domain\":\"github.com\"}",
        )
        .unwrap();

        // Associated / adjacent item (e.g. general documentation or related website)
        let related_site = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/general/utils",
            Some("General Utils"),
            "{\"domain\":\"github.com\"}",
        )
        .unwrap();

        // Link them via item_relationships
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &repo_item.id,
            &related_site.id,
            "co_occurrence",
            0.60,
            None,
        );

        // User explicitly confirms ONLY the primary repository item to PayX
        let conf_ev = ContextFeedbackEvent::confirm_context(&repo_item.id, &ws_payx.id, FeedbackSource::UserExplicitUi);
        ContextFeedbackService::apply_feedback(&conn, &conf_ev).unwrap();

        // Localization invariant:
        // Repository X is confirmed and strongly supported in PayX.
        // But related_site is NOT blindly promoted or automatically confirmed to PayX!
        let loc = ContextPositiveFeedbackEngine::verify_relationship_localization(
            &conn,
            &repo_item.id,
            &related_site.id,
            &ws_payx.id,
        );
        assert!(loc.confirmed_entity_supported);
        assert!(!loc.related_entity_blindly_promoted);
        assert!(loc.is_isolated);

        let related_assess = ContextPositiveFeedbackEngine::evaluate_confirmed_relationship(&conn, &related_site.id, &ws_payx.id);
        assert!(!related_assess.is_confirmed);
        assert_eq!(related_assess.confidence, 0.0);
    }

    #[test]
    fn test_positive_feedback_mark_important_strengthens_memory() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws_payx.id),
            None,
            "document",
            "Stripe API Key Architecture Blueprint",
            Some("API Architecture"),
            "{}",
        )
        .unwrap();

        // Mark as important memory
        let mark_ev = ContextFeedbackEvent::mark_important(
            FeedbackTargetRef::Item(item.id.clone()),
            Some(ws_payx.id.clone()),
            FeedbackSource::MemoryInspectionPanel,
        );
        ContextFeedbackService::apply_feedback(&conn, &mark_ev).unwrap();

        let assess = ContextPositiveFeedbackEngine::evaluate_confirmed_relationship(&conn, &item.id, &ws_payx.id);
        assert!(assess.is_important_memory);
        assert!(assess.is_confirmed);
    }

    #[test]
    fn test_positive_feedback_confirm_task_strengthens_task() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let task_cand = crate::db::models::TaskCandidate::new(
            Some(ws_payx.id.clone()),
            "Deploy Settlement Microservice",
            0.65,
        );
        let created_task = crate::db::operations::create_task_candidate(&conn, &task_cand).unwrap();

        // Confirm task
        let conf_task_ev = ContextFeedbackEvent::confirm_task(&created_task.id, &ws_payx.id, FeedbackSource::UserExplicitUi);
        ContextFeedbackService::apply_feedback(&conn, &conf_task_ev).unwrap();

        let assess = ContextPositiveFeedbackEngine::evaluate_confirmed_relationship(&conn, &created_task.id, &ws_payx.id);
        assert!(assess.is_active_task);
        assert!(assess.is_confirmed);
    }

    #[test]
    fn test_positive_feedback_resume_context_strengthens_lifecycle() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        // Pause/archive first
        let _ = conn.execute("UPDATE workspaces SET lifecycle_state = 'dormant' WHERE id = ?1", rusqlite::params![ws_payx.id]);

        // Resume context
        let resume_ev = ContextFeedbackEvent::resume_context(&ws_payx.id, FeedbackSource::UserExplicitUi);
        let res = ContextFeedbackService::apply_feedback(&conn, &resume_ev).unwrap();
        assert!(res.success);

        let ws = crate::db::operations::get_workspace(&conn, &ws_payx.id).unwrap();
        assert_eq!(ws.lifecycle_state, "active");
    }

    #[test]
    fn test_precedence_tier_ranking_order() {
        use EvidencePriorityTier::*;
        assert!(ExplicitUserConfirmation.has_higher_precedence_than(&ExplicitUserRejection));
        assert!(ExplicitUserRejection.has_higher_precedence_than(&StrongContextualEvidence));
        assert!(StrongContextualEvidence.has_higher_precedence_than(&InferredSemanticSimilarity));
        assert!(InferredSemanticSimilarity.has_higher_precedence_than(&WeakContextualEvidence));
        assert!(WeakContextualEvidence.has_higher_precedence_than(&TemporalProximity));
        assert!(TemporalProximity.has_higher_precedence_than(&ApplicationIdentity));

        assert_eq!(ExplicitUserConfirmation.rank(), 1);
        assert_eq!(ExplicitUserRejection.rank(), 2);
        assert_eq!(StrongContextualEvidence.rank(), 3);
        assert_eq!(InferredSemanticSimilarity.rank(), 4);
        assert_eq!(WeakContextualEvidence.rank(), 5);
        assert_eq!(TemporalProximity.rank(), 6);
        assert_eq!(ApplicationIdentity.rank(), 7);
    }

    #[test]
    fn test_precedence_explicit_confirmation_overrides_older_rejection() {
        let policy = FeedbackPrecedencePolicy::default();
        let now = Utc::now();
        let t0 = (now - chrono::Duration::hours(2)).to_rfc3339();
        let t1 = now.to_rfc3339();

        let old_rejection = PrioritizedEvidenceSignal::new(
            "sig-rej",
            EvidencePriorityTier::ExplicitUserRejection,
            Some("ws-payx".to_string()),
            Some("item-123".to_string()),
            1.0,
            t0,
            "User previously dismissed item from PayX",
        );

        let new_confirmation = PrioritizedEvidenceSignal::new(
            "sig-conf",
            EvidencePriorityTier::ExplicitUserConfirmation,
            Some("ws-payx".to_string()),
            Some("item-123".to_string()),
            1.0,
            t1,
            "User explicitly confirmed item into PayX",
        );

        let res = FeedbackPrecedenceEngine::resolve_conflicting_signals(
            &[old_rejection, new_confirmation],
            &policy,
        )
        .unwrap();

        assert_eq!(res.winning_tier, EvidencePriorityTier::ExplicitUserConfirmation);
        assert_eq!(res.winning_signal.signal_id, "sig-conf");
        assert_eq!(res.effective_verdict, "confirmed_by_user");
    }

    #[test]
    fn test_precedence_explicit_rejection_overrides_inferred_semantic() {
        let policy = FeedbackPrecedencePolicy::default();
        let now = Utc::now().to_rfc3339();

        let rejection = PrioritizedEvidenceSignal::new(
            "sig-rej",
            EvidencePriorityTier::ExplicitUserRejection,
            Some("ws-payx".to_string()),
            Some("item-123".to_string()),
            1.0,
            &now,
            "User rejected item from PayX",
        );

        let high_semantic = PrioritizedEvidenceSignal::new(
            "sig-semantic",
            EvidencePriorityTier::InferredSemanticSimilarity,
            Some("ws-payx".to_string()),
            Some("item-123".to_string()),
            0.82,
            &now,
            "High cosine similarity with workspace items",
        );

        let res = FeedbackPrecedenceEngine::resolve_conflicting_signals(
            &[high_semantic, rejection],
            &policy,
        )
        .unwrap();

        // User explicit rejection strictly dominates inferred semantic similarity
        assert_eq!(res.winning_tier, EvidencePriorityTier::ExplicitUserRejection);
        assert_eq!(res.winning_signal.signal_id, "sig-rej");
        assert_eq!(res.effective_verdict, "rejected_by_user");
    }

    #[test]
    fn test_precedence_app_identity_subordinate_to_semantic_and_temporal() {
        let policy = FeedbackPrecedencePolicy::default();
        let now = Utc::now().to_rfc3339();

        let app_sig = PrioritizedEvidenceSignal::new(
            "sig-app",
            EvidencePriorityTier::ApplicationIdentity,
            Some("ws-payx".to_string()),
            Some("item-123".to_string()),
            0.20,
            &now,
            "Generic browser host application",
        );

        let temporal_sig = PrioritizedEvidenceSignal::new(
            "sig-temporal",
            EvidencePriorityTier::TemporalProximity,
            Some("ws-payx".to_string()),
            Some("item-123".to_string()),
            0.50,
            &now,
            "Captured during active PayX work session",
        );

        let semantic_sig = PrioritizedEvidenceSignal::new(
            "sig-semantic",
            EvidencePriorityTier::InferredSemanticSimilarity,
            Some("ws-payx".to_string()),
            Some("item-123".to_string()),
            0.65,
            &now,
            "Keyword and topic overlap",
        );

        let res = FeedbackPrecedenceEngine::resolve_conflicting_signals(
            &[app_sig, temporal_sig, semantic_sig],
            &policy,
        )
        .unwrap();

        // Semantic (Tier 4) wins over Temporal (Tier 6) and App Identity (Tier 7)
        assert_eq!(res.winning_tier, EvidencePriorityTier::InferredSemanticSimilarity);
        assert_eq!(res.winning_signal.signal_id, "sig-semantic");
    }

    #[test]
    fn test_precedence_deterministic_multi_signal_arbitration() {
        let policy = FeedbackPrecedencePolicy::default();
        let now = Utc::now().to_rfc3339();

        let sigs = vec![
            PrioritizedEvidenceSignal::new("s1", EvidencePriorityTier::ApplicationIdentity, None, None, 0.1, &now, "App"),
            PrioritizedEvidenceSignal::new("s2", EvidencePriorityTier::WeakContextualEvidence, None, None, 0.2, &now, "Weak"),
            PrioritizedEvidenceSignal::new("s3", EvidencePriorityTier::StrongContextualEvidence, None, None, 0.9, &now, "Strong Anchor"),
            PrioritizedEvidenceSignal::new("s4", EvidencePriorityTier::TemporalProximity, None, None, 0.3, &now, "Temporal"),
            PrioritizedEvidenceSignal::new("s5", EvidencePriorityTier::InferredSemanticSimilarity, None, None, 0.5, &now, "Semantic"),
        ];

        let res = FeedbackPrecedenceEngine::resolve_conflicting_signals(&sigs, &policy).unwrap();
        assert_eq!(res.winning_tier, EvidencePriorityTier::StrongContextualEvidence);
        assert_eq!(res.winning_signal.signal_id, "s3");
        assert_eq!(res.overridden_signals.len(), 4);
    }

    #[test]
    fn test_conflict_confirm_then_reject() {
        let now = Utc::now();
        let t0 = (now - chrono::Duration::hours(2)).to_rfc3339();
        let t1 = now.to_rfc3339();

        let ev1 = ContextFeedbackEvent::confirm_context("item-1", "ws-payx", FeedbackSource::UserExplicitUi)
            .with_timestamp(t0);
        let ev2 = ContextFeedbackEvent::reject_context("item-1", "ws-payx", FeedbackSource::InboxAction, None)
            .with_timestamp(t1);

        let res = FeedbackConflictResolver::resolve_events(&[ev1, ev2], "item-1", "ws-payx");
        assert!(res.effective_state.is_rejected());
        assert_eq!(res.total_history_count, 2);
        assert_eq!(res.confirmations_count, 1);
        assert_eq!(res.rejections_count, 1);
        assert_eq!(res.latest_action, Some(FeedbackAction::RejectContext));
        assert!(res.history_preserved);
    }

    #[test]
    fn test_conflict_reject_then_confirm() {
        let now = Utc::now();
        let t0 = (now - chrono::Duration::hours(2)).to_rfc3339();
        let t1 = now.to_rfc3339();

        let ev1 = ContextFeedbackEvent::reject_context("item-1", "ws-payx", FeedbackSource::InboxAction, None)
            .with_timestamp(t0);
        let ev2 = ContextFeedbackEvent::confirm_context("item-1", "ws-payx", FeedbackSource::UserExplicitUi)
            .with_timestamp(t1);

        let res = FeedbackConflictResolver::resolve_events(&[ev1, ev2], "item-1", "ws-payx");
        assert!(res.effective_state.is_confirmed());
        assert_eq!(res.total_history_count, 2);
        assert_eq!(res.confirmations_count, 1);
        assert_eq!(res.rejections_count, 1);
        assert_eq!(res.latest_action, Some(FeedbackAction::ConfirmContext));
    }

    #[test]
    fn test_conflict_confirm_reject_confirm_oscillation() {
        let now = Utc::now();
        let t0 = (now - chrono::Duration::hours(3)).to_rfc3339();
        let t1 = (now - chrono::Duration::hours(2)).to_rfc3339();
        let t2 = now.to_rfc3339();

        // 1. Confirm
        let ev1 = ContextFeedbackEvent::confirm_context("item-1", "ws-payx", FeedbackSource::UserExplicitUi)
            .with_timestamp(t0);
        // 2. Remove / Reject
        let ev2 = ContextFeedbackEvent::remove_from_context("item-1", "ws-payx", FeedbackSource::WorksetTray, None)
            .with_timestamp(t1);
        // 3. Confirm again
        let ev3 = ContextFeedbackEvent::confirm_context("item-1", "ws-payx", FeedbackSource::UserExplicitUi)
            .with_timestamp(t2);

        let res = FeedbackConflictResolver::resolve_events(&[ev1, ev2, ev3], "item-1", "ws-payx");
        assert!(res.effective_state.is_confirmed());
        assert_eq!(res.total_history_count, 3);
        assert_eq!(res.confirmations_count, 2);
        assert_eq!(res.rejections_count, 1);
        assert_eq!(res.latest_action, Some(FeedbackAction::ConfirmContext));
        assert!(res.history_preserved);
    }

    #[test]
    fn test_conflict_multiple_contexts_isolation() {
        let now = Utc::now();
        let t0 = (now - chrono::Duration::hours(1)).to_rfc3339();

        let ev_payx = ContextFeedbackEvent::confirm_context("item-1", "ws-payx", FeedbackSource::UserExplicitUi)
            .with_timestamp(&t0);
        let ev_personal = ContextFeedbackEvent::reject_context("item-1", "ws-personal", FeedbackSource::InboxAction, None)
            .with_timestamp(&t0);

        let events = vec![ev_payx, ev_personal];

        // PayX is confirmed
        let res_payx = FeedbackConflictResolver::resolve_events(&events, "item-1", "ws-payx");
        assert!(res_payx.effective_state.is_confirmed());
        assert_eq!(res_payx.total_history_count, 1);

        // Personal is rejected
        let res_personal = FeedbackConflictResolver::resolve_events(&events, "item-1", "ws-personal");
        assert!(res_personal.effective_state.is_rejected());
        assert_eq!(res_personal.total_history_count, 1);
    }

    #[test]
    fn test_conflict_different_scopes_specificity_rule() {
        let now = Utc::now().to_rfc3339();

        // Broader SourceContext rejection (specificity 2)
        let ev_broad = ContextFeedbackEvent::new(
            FeedbackAction::RejectContext,
            FeedbackTargetRef::Other { entity_type: "source".to_string(), entity_id: "chrome".to_string() },
            Some("ws-payx".to_string()),
            FeedbackSource::InboxAction,
            FeedbackScope::SourceContext { source_id: "chrome".to_string(), context_id: "ws-payx".to_string() },
            None,
        )
        .with_timestamp(&now);

        // Specific ItemContext confirmation (specificity 1)
        let ev_narrow = ContextFeedbackEvent::new(
            FeedbackAction::ConfirmContext,
            FeedbackTargetRef::Item("item-stripe".to_string()),
            Some("ws-payx".to_string()),
            FeedbackSource::UserExplicitUi,
            FeedbackScope::ItemContext { item_id: "item-stripe".to_string(), context_id: "ws-payx".to_string() },
            None,
        )
        .with_timestamp(&now);

        // Narrower ItemContext confirmation wins over broader SourceContext rejection
        let res = FeedbackConflictResolver::resolve_events(&[ev_broad, ev_narrow], "item-stripe", "ws-payx");
        assert!(res.effective_state.is_confirmed());
    }

    #[test]
    fn test_conflict_old_vs_recent_feedback() {
        let now = Utc::now();
        let old_ts = (now - chrono::Duration::days(45)).to_rfc3339();
        let fresh_ts = now.to_rfc3339();

        let old_ev = ContextFeedbackEvent::reject_context("item-1", "ws-payx", FeedbackSource::InboxAction, None)
            .with_timestamp(old_ts);
        let fresh_ev = ContextFeedbackEvent::confirm_context("item-1", "ws-payx", FeedbackSource::UserExplicitUi)
            .with_timestamp(fresh_ts);

        let res = FeedbackConflictResolver::resolve_events(&[old_ev, fresh_ev], "item-1", "ws-payx");
        assert!(res.effective_state.is_confirmed());
    }

    #[test]
    fn test_conflict_genuine_ambiguity_no_fabricated_certainty() {
        let same_ts = Utc::now().to_rfc3339();

        // Two contradictory actions with exact same timestamp and exact same scope specificity
        let ev1 = ContextFeedbackEvent::confirm_context("item-1", "ws-payx", FeedbackSource::UserExplicitUi)
            .with_timestamp(&same_ts);
        let ev2 = ContextFeedbackEvent::reject_context("item-1", "ws-payx", FeedbackSource::InboxAction, None)
            .with_timestamp(&same_ts);

        let res = FeedbackConflictResolver::resolve_events(&[ev1, ev2], "item-1", "ws-payx");
        assert!(res.is_ambiguous);
        assert!(res.effective_state.is_ambiguous());
    }

    #[test]
    fn test_user_task_correction_e2e_reconstruction() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Firebase Integration").unwrap();

        // 1. Initial Inferred Task: "Cloudinary integration"
        let inferred_task = crate::db::models::TaskCandidate::new(
            Some(ws.id.clone()),
            "Cloudinary integration",
            0.75,
        )
        .with_status(crate::db::models::TaskCandidateStatus::Active);
        let created_inferred = crate::db::operations::create_task_candidate(&conn, &inferred_task).unwrap();

        // Verify initial memory picks up the inferred task
        let mem_service = crate::context_engine::memory::ContextMemoryService::new();
        let initial_mem = mem_service.reconstruct_memory(&conn, &ws.id).unwrap();
        assert_eq!(initial_mem.current_task_title.as_deref(), Some("Cloudinary integration"));
        assert_eq!(initial_mem.current_task_confidence, Some(0.75));

        // 2. User applies correction: "No, I was working on Firebase metadata."
        let feedback_service = ContextFeedbackService::new();
        let correct_res = feedback_service
            .correct_task_candidate(
                &conn,
                &ws.id,
                Some(&created_inferred.id),
                "Firebase metadata",
                FeedbackSource::UserExplicitUi,
                Some("User corrected task from Cloudinary integration to Firebase metadata"),
            )
            .unwrap();

        assert!(correct_res.success);
        assert!(correct_res.applied_systems.contains(&"task_candidates".to_string()));

        // 3. Verify previous inference remains historically available (ZERO DELETION)
        let old_candidate = crate::db::operations::get_task_candidate(&conn, &created_inferred.id).unwrap();
        assert_eq!(old_candidate.title, "Cloudinary integration");
        assert_eq!(old_candidate.status, crate::db::models::TaskCandidateStatus::Dismissed);

        // 4. Verify new task candidate exists, is Confirmed with confidence 1.0
        let all_candidates = crate::db::operations::list_task_candidates_for_context(&conn, &ws.id).unwrap();
        assert_eq!(all_candidates.len(), 2, "Historical task candidates must be preserved without deletion");

        let confirmed_candidate = all_candidates
            .iter()
            .find(|c| c.title == "Firebase metadata")
            .expect("New confirmed task candidate must exist");
        assert_eq!(confirmed_candidate.status, crate::db::models::TaskCandidateStatus::Confirmed);
        assert_eq!(confirmed_candidate.confidence, 1.0);

        // 5. Verify Context Memory reconstruction reflects correction
        let updated_mem = mem_service.reconstruct_memory(&conn, &ws.id).unwrap();
        assert_eq!(updated_mem.current_task_title.as_deref(), Some("Firebase metadata"));
        assert_eq!(updated_mem.current_task_confidence, Some(1.0));
        assert_eq!(
            updated_mem.current_task_provenance,
            Some(crate::context_engine::memory::InformationProvenance::Confirmed)
        );

        // 6. Verify Context Reconstruction engine respects user correction
        let recon_service = crate::context_engine::reconstruction::ContextReconstructionService::new();
        let recon = recon_service.reconstruct_context(&conn, &ws.id).unwrap();
        assert_eq!(recon.current_task.status, crate::context_engine::reconstruction::ReconstructionSectionStatus::Available);
        assert_eq!(recon.current_task.title.as_deref(), Some("Firebase metadata"));
        assert_eq!(recon.current_task.confidence, Some(1.0));
    }

    #[test]
    fn test_user_task_confirmation_via_feedback_action() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Payment System").unwrap();

        let task = crate::db::models::TaskCandidate::new(
            Some(ws.id.clone()),
            "Stripe webhook handler",
            0.60,
        );
        let created = crate::db::operations::create_task_candidate(&conn, &task).unwrap();

        let event = ContextFeedbackEvent::confirm_task(
            &created.id,
            &ws.id,
            FeedbackSource::UserExplicitUi,
        );

        let res = ContextFeedbackService::apply_feedback(&conn, &event).unwrap();
        assert!(res.success);

        let updated = crate::db::operations::get_task_candidate(&conn, &created.id).unwrap();
        assert_eq!(updated.status, crate::db::models::TaskCandidateStatus::Confirmed);
        assert_eq!(updated.confidence, 1.0);

        let recon = crate::context_engine::reconstruction::ContextReconstructionService::new()
            .reconstruct_context(&conn, &ws.id)
            .unwrap();
        assert_eq!(recon.current_task.title.as_deref(), Some("Stripe webhook handler"));
        assert_eq!(recon.current_task.confidence, Some(1.0));
    }

    #[test]
    fn test_explicit_lifecycle_resume_and_archive_e2e() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Context Engine Day 25").unwrap();

        // 1. Mark workspace as dormant in DB to simulate old project
        let t_old = (Utc::now() - chrono::Duration::days(45)).to_rfc3339();
        conn.execute(
            "UPDATE workspaces SET lifecycle_state = 'dormant', last_activity_at = ?1 WHERE id = ?2",
            rusqlite::params![t_old, ws.id],
        )
        .unwrap();

        let initial_lc = crate::context_engine::lifecycle::ContextLifecycleService::get_context_lifecycle(&conn, &ws.id).unwrap();
        assert_eq!(initial_lc.lifecycle_state, crate::context_engine::lifecycle::ContextLifecycleState::Dormant);

        // 2. User explicitly resumes context: DORMANT -> ACTIVE
        let feedback_svc = ContextFeedbackService::new();
        let resume_res = feedback_svc
            .resume_context(&conn, &ws.id, FeedbackSource::UserExplicitUi)
            .unwrap();
        assert!(resume_res.success);
        assert!(resume_res.applied_systems.contains(&"workspaces".to_string()));
        assert!(resume_res.applied_systems.contains(&"context_signals".to_string()));

        let ws_after_resume = crate::db::operations::get_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_after_resume.lifecycle_state, "active");
        assert!(ws_after_resume.archived_at.is_none());

        let resumed_lc = crate::context_engine::lifecycle::ContextLifecycleService::get_context_lifecycle(&conn, &ws.id).unwrap();
        assert_eq!(resumed_lc.lifecycle_state, crate::context_engine::lifecycle::ContextLifecycleState::Active);
        assert_eq!(resumed_lc.confidence, 1.0);
        assert!(resumed_lc.last_resumed_at.is_some());
        assert!(resumed_lc.source_evidence.iter().any(|e| e.evidence_type == "user_action"));

        // 3. User explicitly archives context: ACTIVE -> ARCHIVED
        let archive_res = feedback_svc
            .archive_context(
                &conn,
                &ws.id,
                FeedbackSource::UserExplicitUi,
                Some("Project sprint completed and shipped"),
            )
            .unwrap();
        assert!(archive_res.success);

        let ws_after_archive = crate::db::operations::get_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_after_archive.lifecycle_state, "archived");
        assert!(ws_after_archive.archived_at.is_some());

        let archived_lc = crate::context_engine::lifecycle::ContextLifecycleService::get_context_lifecycle(&conn, &ws.id).unwrap();
        assert_eq!(archived_lc.lifecycle_state, crate::context_engine::lifecycle::ContextLifecycleState::Archived);
        assert_eq!(archived_lc.confidence, 1.0);
        assert!(!archived_lc.lifecycle_state.allows_automated_matching());

        // 4. User restores/reopens archived context: ARCHIVED -> ACTIVE
        let restore_res = feedback_svc
            .restore_context(&conn, &ws.id, FeedbackSource::UserExplicitUi)
            .unwrap();
        assert!(restore_res.success);

        let ws_restored = crate::db::operations::get_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_restored.lifecycle_state, "active");
        assert!(ws_restored.archived_at.is_none());

        let restored_lc = crate::context_engine::lifecycle::ContextLifecycleService::get_context_lifecycle(&conn, &ws.id).unwrap();
        assert_eq!(restored_lc.lifecycle_state, crate::context_engine::lifecycle::ContextLifecycleState::Active);
        assert_eq!(restored_lc.confidence, 1.0);
    }

    #[test]
    fn test_archived_context_preserves_all_items_and_tasks_without_deletion() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Secure Vault Project").unwrap();

        // Add items
        let item1 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "code",
            "fn encrypt_data() {}",
            Some("Vault Encryption Module"),
            "{}",
        )
        .unwrap();

        let item2 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://vault.example.com/docs",
            Some("Vault Documentation"),
            "{}",
        )
        .unwrap();

        // Add task candidates
        let task1 = crate::db::models::TaskCandidate::new(
            Some(ws.id.clone()),
            "Implement AES256-GCM cipher",
            0.90,
        );
        let created_task = crate::db::operations::create_task_candidate(&conn, &task1).unwrap();

        // User archives context
        let feedback_svc = ContextFeedbackService::new();
        let res = feedback_svc
            .archive_context(&conn, &ws.id, FeedbackSource::UserExplicitUi, Some("Archiving vault project"))
            .unwrap();
        assert!(res.success);

        // INVARIANT VERIFICATION: Archived DOES NOT mean deleted!
        let fetched_item1 = crate::db::operations::get_item(&conn, &item1.id).unwrap();
        assert_eq!(fetched_item1.title.as_deref(), Some("Vault Encryption Module"));

        let fetched_item2 = crate::db::operations::get_item(&conn, &item2.id).unwrap();
        assert_eq!(fetched_item2.title.as_deref(), Some("Vault Documentation"));

        let all_tasks = crate::db::operations::list_task_candidates_for_context(&conn, &ws.id).unwrap();
        assert_eq!(all_tasks.len(), 1);
        assert_eq!(all_tasks[0].id, created_task.id);
        assert_eq!(all_tasks[0].title, "Implement AES256-GCM cipher");

        // Context reconstruction should still work on archived context
        let recon = crate::context_engine::reconstruction::ContextReconstructionService::new()
            .reconstruct_cold(&conn, &ws.id)
            .unwrap();
        assert_eq!(recon.context.lifecycle_state, Some("archived".to_string()));
    }

    #[test]
    fn test_deterministic_adaptation_generic_chrome_rejection_dampens_future_generic_pages() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        // 1. Create two generic Chrome search items
        let generic_item1 = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://www.google.com/search?q=weather+in+tokyo",
            Some("Google Search: weather in tokyo"),
            r#"{"app_name": "Google Chrome", "domain": "google.com", "url": "https://www.google.com/search?q=weather+in+tokyo"}"#,
        )
        .unwrap();

        let generic_item2 = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://www.google.com/search?q=best+coffee+shops",
            Some("Google Search: best coffee shops"),
            r#"{"app_name": "Google Chrome", "domain": "google.com", "url": "https://www.google.com/search?q=best+coffee+shops"}"#,
        )
        .unwrap();

        // 2. Reject both generic items from PayX context
        let ev1 = ContextFeedbackEvent::remove_from_context(
            &generic_item1.id,
            &ws.id,
            FeedbackSource::UserExplicitUi,
            Some("Not related to PayX".to_string()),
        );
        ContextFeedbackService::apply_feedback(&conn, &ev1).unwrap();

        let ev2 = ContextFeedbackEvent::remove_from_context(
            &generic_item2.id,
            &ws.id,
            FeedbackSource::UserExplicitUi,
            Some("Random search".to_string()),
        );
        ContextFeedbackService::apply_feedback(&conn, &ev2).unwrap();

        // 3. New generic Chrome page arrives with base score 0.55
        let generic_item3 = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://www.google.com/search?q=flights+to+london",
            Some("Google Search: flights to london"),
            r#"{"app_name": "Google Chrome", "domain": "google.com", "url": "https://www.google.com/search?q=flights+to+london"}"#,
        )
        .unwrap();

        let assessment = ContextDeterministicAdaptationEngine::evaluate_adaptation(
            &conn,
            &generic_item3,
            &ws,
            0.55,
        );

        assert_eq!(assessment.verdict, AdaptationVerdict::GenericAppDampened);
        assert!(assessment.is_generic_suppressed);
        assert!(!assessment.has_specific_entity_match);
        assert_eq!(assessment.app_rejection_count, 2);
        assert!(assessment.adapted_score <= 0.30, "Generic page should be dampened to <= 0.30, got {}", assessment.adapted_score);
        assert!(assessment.adapted_score < assessment.base_score);
    }

    #[test]
    fn test_deterministic_adaptation_specific_payx_repo_recognized_despite_chrome_rejections() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        // 1. Record 2 generic Chrome rejections for PayX
        let generic_item1 = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://www.google.com/search?q=funny+memes",
            Some("Google Search: funny memes"),
            r#"{"app_name": "Google Chrome", "domain": "google.com", "url": "https://www.google.com/search?q=funny+memes"}"#,
        )
        .unwrap();
        let generic_item2 = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://reddit.com/r/rust",
            Some("Reddit Rust"),
            r#"{"app_name": "Google Chrome", "domain": "reddit.com", "url": "https://reddit.com/r/rust"}"#,
        )
        .unwrap();

        let ev1 = ContextFeedbackEvent::remove_from_context(&generic_item1.id, &ws.id, FeedbackSource::UserExplicitUi, None);
        ContextFeedbackService::apply_feedback(&conn, &ev1).unwrap();

        let ev2 = ContextFeedbackEvent::remove_from_context(&generic_item2.id, &ws.id, FeedbackSource::UserExplicitUi, None);
        ContextFeedbackService::apply_feedback(&conn, &ev2).unwrap();

        // 2. Specific PayX GitHub repository URL in Chrome arrives
        let payx_repo_item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/payx/core-payment-gateway",
            Some("PayX Core Payment Gateway - PR #128"),
            r#"{"app_name": "Google Chrome", "domain": "github.com", "url": "https://github.com/payx/core-payment-gateway", "repo_slug": "payx/core-payment-gateway"}"#,
        )
        .unwrap();

        let assessment = ContextDeterministicAdaptationEngine::evaluate_adaptation(
            &conn,
            &payx_repo_item,
            &ws,
            0.70,
        );

        // Invariant: Specific relationship recognized, bypassing generic app dampening!
        assert_eq!(assessment.verdict, AdaptationVerdict::SpecificRelationshipRecognized);
        assert!(assessment.has_specific_entity_match);
        assert!(!assessment.is_generic_suppressed);
        assert!(assessment.adapted_score >= 0.85, "Adapted score should be >= 0.85, got {}", assessment.adapted_score);
        assert!(assessment.confidence >= 0.90, "Confidence should be >= 0.90, got {}", assessment.confidence);
    }

    #[test]
    fn test_deterministic_adaptation_cross_workspace_isolation() {
        let conn = setup_test_db();
        let ws_payx = crate::db::operations::create_workspace(&conn, "PayX").unwrap();
        let ws_personal = crate::db::operations::create_workspace(&conn, "Personal Notes").unwrap();

        // Reject 3 generic Chrome items from PayX
        for i in 0..3 {
            let item = crate::db::operations::create_item(
                &conn,
                None,
                None,
                "url",
                &format!("https://google.com/search?q=query_{}", i),
                Some(&format!("Query {}", i)),
                r#"{"app_name": "Google Chrome", "domain": "google.com"}"#,
            )
            .unwrap();

            let ev = ContextFeedbackEvent::remove_from_context(&item.id, &ws_payx.id, FeedbackSource::UserExplicitUi, None);
            ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();
        }

        // New generic item evaluated against Personal Notes workspace
        let new_item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://google.com/search?q=grocery+recipes",
            Some("Grocery Recipes"),
            r#"{"app_name": "Google Chrome", "domain": "google.com"}"#,
        )
        .unwrap();

        let assessment = ContextDeterministicAdaptationEngine::evaluate_adaptation(
            &conn,
            &new_item,
            &ws_personal,
            0.50,
        );

        // PayX rejections must NOT affect Personal Notes!
        assert_eq!(assessment.verdict, AdaptationVerdict::UnmodifiedBaseline);
        assert!(!assessment.is_generic_suppressed);
        assert_eq!(assessment.app_rejection_count, 0);
        assert_eq!(assessment.adapted_score, 0.50);
    }

    #[test]
    fn test_deterministic_adaptation_decay_over_time() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://docs.example.com/api",
            Some("Example API Docs"),
            r#"{"app_name": "Google Chrome", "domain": "docs.example.com"}"#,
        )
        .unwrap();

        // Direct rejection recorded 120 days ago
        let old_time = Utc::now() - chrono::Duration::days(120);
        let old_time_str = old_time.to_rfc3339();

        let payload = serde_json::json!({
            "action": "user_removed_from_context",
            "item_id": item.id,
            "workspace_id": ws.id,
            "app_name": "Google Chrome",
            "reason": "Not related",
            "recorded_at": old_time_str
        }).to_string();

        conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'user_correction', NULL, ?4, ?5, ?5)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), ws.id, item.id, payload, old_time_str],
        ).unwrap();

        let assessment = ContextDeterministicAdaptationEngine::evaluate_adaptation_with_time(
            &conn,
            &item,
            &ws,
            0.60,
            Utc::now(),
        );

        assert_eq!(assessment.verdict, AdaptationVerdict::DecayedHistoricalPenalty);
        assert_eq!(assessment.decay_factor, 0.40);
        // Base penalty 0.40 * decay 0.40 = 0.16; adapted score = 0.60 - 0.16 = 0.44
        assert!((assessment.adapted_score - 0.44).abs() < 0.01);
    }

    #[test]
    fn test_permanent_preference_longevity_across_time() {
        let now = Utc::now();
        let config = FeedbackRelevanceConfig::default();

        // Archive Context is a Permanent Preference
        let assessment_10d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::ArchiveContext,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(10),
            now,
            &config,
        );
        assert_eq!(assessment_10d.persistence_category, FeedbackPersistenceCategory::PermanentPreference);
        assert_eq!(assessment_10d.decay_factor, 1.0);
        assert!(assessment_10d.is_active);
        assert!(!assessment_10d.is_expired);

        let assessment_100d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::ArchiveContext,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(100),
            now,
            &config,
        );
        assert_eq!(assessment_100d.decay_factor, 1.0);
        assert!(assessment_100d.is_active);
        assert!(!assessment_100d.is_expired);

        let assessment_365d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::MarkImportant,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(365),
            now,
            &config,
        );
        assert_eq!(assessment_365d.decay_factor, 1.0);
        assert!(assessment_365d.is_active);
        assert!(!assessment_365d.is_expired);
    }

    #[test]
    fn test_contextual_correction_durability_and_decay_floor() {
        let now = Utc::now();
        let config = FeedbackRelevanceConfig::default();

        // "Remove this random item from PayX" -> ContextualCorrection
        let assess_30d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::RemoveFromContext,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(30),
            now,
            &config,
        );
        assert_eq!(assess_30d.persistence_category, FeedbackPersistenceCategory::ContextualCorrection);
        assert_eq!(assess_30d.decay_factor, 1.0);
        assert!(assess_30d.is_active);
        assert!(!assess_30d.is_expired);

        // At 90 days (halfway through 60-day decay window from 60..120 days) -> factor 0.70
        let assess_90d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::RemoveFromContext,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(90),
            now,
            &config,
        );
        assert!((assess_90d.decay_factor - 0.70).abs() < 0.01);
        assert!(assess_90d.is_active);
        assert!(!assess_90d.is_expired);

        // At 180 days -> firmly at anchor floor (0.40), never expired
        let assess_180d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::RemoveFromContext,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(180),
            now,
            &config,
        );
        assert_eq!(assess_180d.decay_factor, 0.40);
        assert!(assess_180d.is_active);
        assert!(!assess_180d.is_expired);
    }

    #[test]
    fn test_temporary_dismissal_cooldown_and_expiration() {
        let now = Utc::now();
        let config = FeedbackRelevanceConfig::default();

        // "Dismiss this suggestion" -> TemporaryDismissal (14 day cooldown)
        let assess_7d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::DismissSuggestion,
            FeedbackSource::InboxAction,
            now - chrono::Duration::days(7),
            now,
            &config,
        );
        assert_eq!(assess_7d.persistence_category, FeedbackPersistenceCategory::TemporaryDismissal);
        assert!((assess_7d.decay_factor - 0.50).abs() < 0.01);
        assert!(assess_7d.is_active);
        assert!(!assess_7d.is_expired);

        // At 15 days -> expired beyond 14-day window
        let assess_15d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::DismissSuggestion,
            FeedbackSource::InboxAction,
            now - chrono::Duration::days(15),
            now,
            &config,
        );
        assert_eq!(assess_15d.decay_factor, 0.0);
        assert!(!assess_15d.is_active);
        assert!(assess_15d.is_expired);
    }

    #[test]
    fn test_historical_inferred_feedback_rapid_decay() {
        let now = Utc::now();
        let config = FeedbackRelevanceConfig::default();

        // Inferred feedback from implicit heuristics
        let assess_7d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::DismissSuggestion,
            FeedbackSource::ImplicitInference,
            now - chrono::Duration::days(7),
            now,
            &config,
        );
        assert_eq!(assess_7d.persistence_category, FeedbackPersistenceCategory::HistoricalInferred);
        // 7 days = 1 half-life -> 0.50
        assert!((assess_7d.decay_factor - 0.50).abs() < 0.01);
        assert!(assess_7d.is_active);

        // 28 days = 4 half-lives -> (0.5)^4 = 0.0625
        let assess_28d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::DismissSuggestion,
            FeedbackSource::ImplicitInference,
            now - chrono::Duration::days(28),
            now,
            &config,
        );
        assert!((assess_28d.decay_factor - 0.0625).abs() < 0.001);
        assert!(assess_28d.is_active);

        // 35 days = 5 half-lives -> 0.03125 (< 0.05 threshold) -> expired
        let assess_35d = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::DismissSuggestion,
            FeedbackSource::ImplicitInference,
            now - chrono::Duration::days(35),
            now,
            &config,
        );
        assert!((assess_35d.decay_factor - 0.03125).abs() < 0.001);
        assert!(!assess_35d.is_active);
        assert!(assess_35d.is_expired);
    }

    #[test]
    fn test_configurable_relevance_policy_custom_settings() {
        let now = Utc::now();
        let custom_config = FeedbackRelevanceConfig {
            permanent_retention_weight: 0.90,
            contextual_correction_active_days: 10.0,
            contextual_correction_decay_window_days: 20.0,
            contextual_correction_floor: 0.15,
            temporary_dismissal_cooldown_days: 5.0,
            historical_inferred_half_life_days: 3.0,
        };

        // Temporary dismissal with 5-day window evaluated at 6 days
        let assess = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::DismissSuggestion,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(6),
            now,
            &custom_config,
        );
        assert!(assess.is_expired);
        assert_eq!(assess.decay_factor, 0.0);

        // Contextual correction at 50 days (beyond 10 + 20 = 30 days) -> floor 0.15
        let assess_corr = ContextFeedbackRelevanceEngine::evaluate_action_relevance(
            FeedbackAction::RejectContext,
            FeedbackSource::UserExplicitUi,
            now - chrono::Duration::days(50),
            now,
            &custom_config,
        );
        assert_eq!(assess_corr.decay_factor, 0.15);
    }

    #[test]
    fn test_feedback_inspector_target_inspection_prompt_example() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://payx.internal/docs",
            Some("Item X - PayX Documentation"),
            r#"{"app_name": "Google Chrome", "domain": "payx.internal"}"#,
        )
        .unwrap();

        // User feedback: Removed from context
        let ev = ContextFeedbackEvent::remove_from_context(
            &item.id,
            &ws.id,
            FeedbackSource::UserExplicitUi,
            Some("Removed from context".to_string()),
        );
        ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();

        // Inspect target
        let report = ContextFeedbackInspector::inspect_target(&conn, &item.id, Some(&ws.id)).unwrap();

        assert_eq!(report.target_id, item.id);
        assert_eq!(report.context_name.as_deref(), Some("PayX"));
        assert_eq!(report.effective_state, "Rejected");
        assert_eq!(report.history_entries.len(), 1);

        let entry = &report.history_entries[0];
        assert_eq!(entry.action, "remove_from_context");
        assert_eq!(entry.action_display, "Remove from Context");
        assert_eq!(entry.scope, "item_context");
        assert_eq!(entry.effective_state, "Rejected");
        assert!(entry.is_active);

        let card = ContextFeedbackInspector::format_entry_card(entry);
        assert!(card.contains("Target:"));
        assert!(card.contains("PayX"));
        assert!(card.contains("Remove from Context"));
        assert!(card.contains("item_context"));
        assert!(card.contains("Rejected"));
    }

    #[test]
    fn test_feedback_inspector_context_level_report() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        let item1 = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://payx.com/api",
            Some("PayX API"),
            "{}",
        )
        .unwrap();

        let item2 = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://google.com/search?q=random",
            Some("Random Search"),
            "{}",
        )
        .unwrap();

        // 1 confirmation + 1 rejection
        let ev1 = ContextFeedbackEvent::confirm_context(&item1.id, &ws.id, FeedbackSource::UserExplicitUi);
        ContextFeedbackService::apply_feedback(&conn, &ev1).unwrap();

        let ev2 = ContextFeedbackEvent::remove_from_context(&item2.id, &ws.id, FeedbackSource::UserExplicitUi, None);
        ContextFeedbackService::apply_feedback(&conn, &ev2).unwrap();

        let report = ContextFeedbackInspector::inspect_context(&conn, &ws.id).unwrap();

        assert_eq!(report.context_id, ws.id);
        assert_eq!(report.context_name, "PayX");
        assert_eq!(report.total_feedback_events, 2);
        assert_eq!(report.confirmations_count, 1);
        assert_eq!(report.rejections_count, 1);
        assert_eq!(report.active_feedback_events, 2);
    }

    #[test]
    fn test_feedback_inspector_privacy_sanitization() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Security Project").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Authorization token: sk-live-secret-key-1234567890abcdef",
            Some("API Key sk-live-secret-key-1234567890abcdef"),
            "{}",
        )
        .unwrap();

        let ev = ContextFeedbackEvent::remove_from_context(&item.id, &ws.id, FeedbackSource::UserExplicitUi, None);
        ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();

        let report = ContextFeedbackInspector::inspect_target(&conn, &item.id, Some(&ws.id)).unwrap();

        // Secrets must be sanitized from target label
        assert!(!report.target_label.contains("sk-live-secret-key-1234567890abcdef"));
        assert!(report.target_label.contains("[API_KEY_REDACTED]") || report.target_label.contains("REDACTED"));
    }

    #[test]
    fn test_feedback_inspector_conflicting_events_diagnostic() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "PayX").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://payx.internal/service",
            Some("PayX Service"),
            "{}",
        )
        .unwrap();

        // Confirm first
        let ev1 = ContextFeedbackEvent::confirm_context(&item.id, &ws.id, FeedbackSource::UserExplicitUi)
            .with_timestamp((Utc::now() - chrono::Duration::hours(2)).to_rfc3339());
        ContextFeedbackService::apply_feedback(&conn, &ev1).unwrap();

        // Later rejected
        let ev2 = ContextFeedbackEvent::remove_from_context(&item.id, &ws.id, FeedbackSource::UserExplicitUi, None)
            .with_timestamp((Utc::now() - chrono::Duration::minutes(5)).to_rfc3339());
        ContextFeedbackService::apply_feedback(&conn, &ev2).unwrap();

        let report = ContextFeedbackInspector::inspect_target(&conn, &item.id, Some(&ws.id)).unwrap();

        assert_eq!(report.effective_state, "Rejected");
        assert_eq!(report.history_entries.len(), 2);
        assert!(report.history_entries[0].has_conflict);
        assert!(report.history_entries[1].has_conflict);
        assert_eq!(report.competing_conflicts.len(), 1);
        assert_eq!(report.competing_conflicts[0].action, "confirm_context");
    }
}

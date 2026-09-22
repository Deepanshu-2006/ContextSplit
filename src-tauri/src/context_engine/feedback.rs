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
}

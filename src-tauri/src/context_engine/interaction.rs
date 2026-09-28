use serde::{Deserialize, Serialize};
use serde_json::Value;

// =========================================================================
// Interaction Event Types & Source Surfaces
// =========================================================================

/// Supported types of cross-platform context interaction events matching TypeScript InteractionEventType.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InteractionEventType {
    #[serde(rename = "context.open")]
    ContextOpen,
    #[serde(rename = "context.resume")]
    ContextResume,
    #[serde(rename = "context.pause")]
    ContextPause,
    #[serde(rename = "context.capture")]
    ContextCapture,
    #[serde(rename = "context.assign")]
    ContextAssign,
    #[serde(rename = "context.search")]
    ContextSearch,
    #[serde(rename = "context.preview")]
    ContextPreview,
    #[serde(rename = "context.mark_important")]
    ContextMarkImportant,
    #[serde(rename = "context.create")]
    ContextCreate,
    #[serde(rename = "context.switch")]
    ContextSwitch,
}

impl InteractionEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContextOpen => "context.open",
            Self::ContextResume => "context.resume",
            Self::ContextPause => "context.pause",
            Self::ContextCapture => "context.capture",
            Self::ContextAssign => "context.assign",
            Self::ContextSearch => "context.search",
            Self::ContextPreview => "context.preview",
            Self::ContextMarkImportant => "context.mark_important",
            Self::ContextCreate => "context.create",
            Self::ContextSwitch => "context.switch",
        }
    }
}

/// Valid source surfaces from which an interaction event can originate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionSourceSurface {
    App,
    Launcher,
    MacNotch,
    WindowsEdge,
    Keyboard,
    DragDrop,
    FutureAutomation,
}

impl InteractionSourceSurface {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Launcher => "launcher",
            Self::MacNotch => "mac_notch",
            Self::WindowsEdge => "windows_edge",
            Self::Keyboard => "keyboard",
            Self::DragDrop => "drag_drop",
            Self::FutureAutomation => "future_automation",
        }
    }
}

/// Common payload structure for a platform-independent context interaction event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInteractionEvent {
    #[serde(rename = "type")]
    pub event_type: InteractionEventType,
    pub timestamp: String,
    pub source_surface: InteractionSourceSurface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

// =========================================================================
// Interaction Outcomes & Result Payloads
// =========================================================================

/// The standard outcomes of any context interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum InteractionOutcome {
    Success,
    Pending,
    Ambiguous,
    Rejected,
    Error,
    Cancelled,
}

impl InteractionOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Pending => "PENDING",
            Self::Ambiguous => "AMBIGUOUS",
            Self::Rejected => "REJECTED",
            Self::Error => "ERROR",
            Self::Cancelled => "CANCELLED",
        }
    }
}

/// Structured response payload that the Context Engine returns to an
/// interaction surface after a capture or command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInteractionResult {
    pub outcome: InteractionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_context_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl ContextInteractionResult {
    pub fn success(message: impl Into<String>, context_id: Option<String>) -> Self {
        Self {
            outcome: InteractionOutcome::Success,
            message: Some(message.into()),
            resolved_context_id: context_id,
            suggested_context_ids: None,
            metadata: None,
        }
    }

    pub fn ambiguous(suggested_context_ids: Vec<String>, message: impl Into<String>) -> Self {
        Self {
            outcome: InteractionOutcome::Ambiguous,
            message: Some(message.into()),
            resolved_context_id: None,
            suggested_context_ids: Some(suggested_context_ids),
            metadata: None,
        }
    }

    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            outcome: InteractionOutcome::Rejected,
            message: Some(reason.into()),
            resolved_context_id: None,
            suggested_context_ids: None,
            metadata: None,
        }
    }

    pub fn error(error_message: impl Into<String>) -> Self {
        Self {
            outcome: InteractionOutcome::Error,
            message: Some(error_message.into()),
            resolved_context_id: None,
            suggested_context_ids: None,
            metadata: None,
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
    fn test_interaction_event_serde_roundtrip() {
        let json_input = r#"{
            "type": "context.capture",
            "timestamp": "2026-09-29T10:00:00Z",
            "sourceSurface": "mac_notch",
            "contextReference": "ws_contextsplit",
            "targetReference": "item_123",
            "metadata": { "pullX": 5.2 }
        }"#;

        let event: ContextInteractionEvent = serde_json::from_str(json_input).expect("Deserialization failed");
        assert_eq!(event.event_type, InteractionEventType::ContextCapture);
        assert_eq!(event.source_surface, InteractionSourceSurface::MacNotch);
        assert_eq!(event.context_reference.as_deref(), Some("ws_contextsplit"));
        assert_eq!(event.target_reference.as_deref(), Some("item_123"));

        let serialized = serde_json::to_string(&event).expect("Serialization failed");
        assert!(serialized.contains(r#""type":"context.capture""#));
        assert!(serialized.contains(r#""sourceSurface":"mac_notch""#));
        assert!(serialized.contains(r#""contextReference":"ws_contextsplit""#));
    }

    #[test]
    fn test_interaction_result_serde_roundtrip() {
        let result = ContextInteractionResult::success("Item captured", Some("ws_payx".to_string()));
        let json_str = serde_json::to_string(&result).expect("Serialization failed");

        assert!(json_str.contains(r#""outcome":"SUCCESS""#));
        assert!(json_str.contains(r#""resolvedContextId":"ws_payx""#));

        let deserialized: ContextInteractionResult = serde_json::from_str(&json_str).expect("Deserialization failed");
        assert_eq!(deserialized.outcome, InteractionOutcome::Success);
        assert_eq!(deserialized.resolved_context_id.as_deref(), Some("ws_payx"));
        assert_eq!(deserialized.message.as_deref(), Some("Item captured"));
    }

    #[test]
    fn test_interaction_result_ambiguous_and_rejected() {
        let amb = ContextInteractionResult::ambiguous(vec!["ws_1".to_string(), "ws_2".to_string()], "Choose workspace");
        let amb_json = serde_json::to_string(&amb).unwrap();
        assert!(amb_json.contains(r#""outcome":"AMBIGUOUS""#));
        assert!(amb_json.contains(r#""suggestedContextIds":["ws_1","ws_2"]"#));

        let rej = ContextInteractionResult::rejected("Accidental capture filtered by QualityGuard");
        let rej_json = serde_json::to_string(&rej).unwrap();
        assert!(rej_json.contains(r#""outcome":"REJECTED""#));
        assert!(rej_json.contains("QualityGuard"));
    }
}

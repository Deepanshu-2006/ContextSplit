use super::engine::{ContextProvider, ContextEvent, ContextSignalParams};
use serde_json::json;

pub struct CaptureContextProvider;

impl ContextProvider for CaptureContextProvider {
    fn name(&self) -> &'static str {
        "capture_context"
    }

    fn collect(&self, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
        if let Some(ContextEvent::Capture { item_id, workspace_id, source, data_type, metadata }) = event {
            vec![ContextSignalParams {
                workspace_id: workspace_id.clone(),
                item_id: Some(item_id.clone()),
                signal_type: "capture_event".to_string(),
                payload: json!({
                    "capture_method": source,
                    "content_type": data_type,
                    "source_metadata": metadata,
                }),
            }]
        } else {
            vec![]
        }
    }
}

pub struct UserActionContextProvider;

impl ContextProvider for UserActionContextProvider {
    fn name(&self) -> &'static str {
        "user_action_context"
    }

    fn collect(&self, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
        if let Some(ContextEvent::UserAction { item_id, action, source_workspace, destination_workspace }) = event {
            vec![ContextSignalParams {
                workspace_id: destination_workspace.clone(), // Use destination as the primary workspace context
                item_id: Some(item_id.clone()),
                signal_type: "user_action".to_string(),
                payload: json!({
                    "action": action,
                    "source_workspace": source_workspace,
                    "destination_workspace": destination_workspace,
                }),
            }]
        } else {
            vec![]
        }
    }
}

pub struct WorkspaceContextProvider;

impl ContextProvider for WorkspaceContextProvider {
    fn name(&self) -> &'static str {
        "workspace_context"
    }

    fn collect(&self, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
        if let Some(ContextEvent::WorkspaceSelected { workspace_id }) = event {
            vec![ContextSignalParams {
                workspace_id: Some(workspace_id.clone()),
                item_id: None,
                signal_type: "workspace_selected".to_string(),
                payload: json!({
                    "action": "selected"
                }),
            }]
        } else {
            vec![]
        }
    }
}

pub struct ActiveAppProvider;

impl ContextProvider for ActiveAppProvider {
    fn name(&self) -> &'static str {
        "active_app"
    }

    fn collect(&self, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
        if let Some(ContextEvent::Capture { item_id, workspace_id, .. }) = event {
            if let Some(app_info) = super::get_active_app() {
                return vec![ContextSignalParams {
                    workspace_id: workspace_id.clone(),
                    item_id: Some(item_id.clone()),
                    signal_type: "active_app".to_string(),
                    payload: json!({
                        "app_name": app_info.app_name,
                        "process_name": app_info.process_name,
                        "process_id": app_info.process_id,
                        "window_title": app_info.window_title,
                    }),
                }];
            }
        }
        vec![]
    }
}

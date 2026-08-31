use super::engine::{ContextProvider, ContextEvent, ContextSignalParams};
use serde_json::json;

pub struct CaptureContextProvider;

impl ContextProvider for CaptureContextProvider {
    fn name(&self) -> &'static str {
        "capture_context"
    }

    fn collect(&self, _conn: &rusqlite::Connection, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
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

    fn collect(&self, _conn: &rusqlite::Connection, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
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

    fn collect(&self, _conn: &rusqlite::Connection, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
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

    fn collect(&self, conn: &rusqlite::Connection, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
        let settings = crate::db::settings::get_settings(conn).unwrap_or(crate::db::settings::PrivacySettings {
            enable_active_app: false,
            enable_window_title: false,
            enable_browser_domain: false,
            enable_session_tracking: false,
        });

        if !settings.enable_active_app {
            return vec![];
        }

        if let Some(ContextEvent::Capture { item_id, workspace_id, .. }) = event {
            if let Some(app_info) = super::get_active_app() {
                let mut payload = json!({
                    "app_name": app_info.app_name,
                    "process_name": app_info.process_name,
                    "process_id": app_info.process_id,
                });
                
                if settings.enable_window_title {
                    payload.as_object_mut().unwrap().insert("window_title".to_string(), json!(app_info.window_title));
                }

                return vec![ContextSignalParams {
                    workspace_id: workspace_id.clone(),
                    item_id: Some(item_id.clone()),
                    signal_type: "active_app".to_string(),
                    payload,
                }];
            }
        }
        vec![]
    }
}

pub struct UrlContextProvider;

impl ContextProvider for UrlContextProvider {
    fn name(&self) -> &'static str {
        "url_context"
    }

    fn collect(&self, conn: &rusqlite::Connection, event: Option<&ContextEvent>) -> Vec<ContextSignalParams> {
        let settings = crate::db::settings::get_settings(conn).unwrap_or(crate::db::settings::PrivacySettings {
            enable_active_app: false,
            enable_window_title: false,
            enable_browser_domain: false,
            enable_session_tracking: false,
        });

        if !settings.enable_browser_domain {
            return vec![];
        }

        if let Some(ContextEvent::Capture { item_id, workspace_id, metadata, .. }) = event {
            let mut raw_url = None;
            
            if let Some(url_val) = metadata.get("original_url").and_then(|v| v.as_str()) {
                if !url_val.is_empty() {
                    raw_url = Some(url_val.to_string());
                }
            }
            
            if raw_url.is_none() {
                if let Some(content_val) = metadata.get("content").and_then(|v| v.as_str()) {
                    if !content_val.is_empty() {
                        raw_url = Some(content_val.to_string());
                    }
                }
            }
            
            if let Some(mut url_str) = raw_url {
                url_str = url_str.trim().to_string();
                
                // Fallback for malformed URLs (e.g. "example.com")
                let parsed = url::Url::parse(&url_str)
                    .or_else(|_| url::Url::parse(&format!("http://{}", url_str)));
                
                if let Ok(mut u) = parsed {
                    let scheme = u.scheme().to_string();
                    let host = u.host_str().unwrap_or("").to_string();
                    let path = u.path().to_string();
                    let has_query = u.query().is_some();
                    
                    let domain = if host.starts_with("www.") {
                        host[4..].to_string()
                    } else {
                        host.clone()
                    };
                    
                    let url_type = match scheme.as_str() {
                        "https" => "secure_web",
                        "http" => "web",
                        "ftp" | "sftp" => "ftp",
                        "file" => "local",
                        _ => "other"
                    };
                    
                    // Normalize: remove fragments
                    u.set_fragment(None);
                    let normalized_url = u.to_string();
                    
                    return vec![ContextSignalParams {
                        workspace_id: workspace_id.clone(),
                        item_id: Some(item_id.clone()),
                        signal_type: "url_metadata".to_string(),
                        payload: json!({
                            "scheme": scheme,
                            "host": host,
                            "domain": domain,
                            "path": path,
                            "has_query": has_query,
                            "url_type": url_type,
                            "normalized_url": normalized_url,
                        }),
                    }];
                }
            }
        }
        vec![]
    }
}

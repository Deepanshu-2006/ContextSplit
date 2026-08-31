use tauri::{AppHandle, Manager, Emitter};
use serde_json::json;
use crate::{AppState, db::operations};
use super::target::CaptureRequest;

pub fn process_capture(app_handle: AppHandle, req: CaptureRequest) -> Result<(), String> {
    // 1. Serialize metadata
    let metadata = json!({
        "source": req.source,
        "filename": req.filename,
        "mime_type": req.mime_type,
        "original_url": req.original_url,
        "size": req.size
    }).to_string();

    let title_opt = req.title.as_deref();

    let state = app_handle.state::<AppState>();
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    
    // Determine active session
    let session_id_opt = if let Ok(ce) = state.context_engine.lock() {
        ce.get_or_update_active_session(&conn).ok()
    } else {
        None
    };
    
    let item = operations::create_item(
        &conn,
        None, // Inbox
        session_id_opt.as_deref(),
        &req.data_type,
        &req.content,
        title_opt,
        &metadata,
    ).map_err(|e| e.to_string())?;

    // --- Collect Context Signals ---
    let state = app_handle.state::<crate::AppState>();
    if let Ok(ce) = state.context_engine.lock() {
        let metadata = serde_json::json!({
            "filename": req.filename,
            "mime_type": req.mime_type,
            "original_url": req.original_url,
            "size": req.size,
            "content": if req.data_type == "url" { Some(req.content.clone()) } else { None }
        });
        
        ce.process_event(&conn, crate::context_engine::engine::ContextEvent::Capture {
            item_id: item.id.clone(),
            workspace_id: req.workspace_id.clone(),
            source: req.source.clone().unwrap_or_else(|| "unknown".to_string()),
            data_type: req.data_type.clone(),
            metadata,
        });
    }

    // Drop the mutex guard before emitting events or showing notifications to prevent deadlocks
    drop(conn);

    // 3. Emit Tauri Event
    let _ = app_handle.emit("clipboard://new-item", &item);

    // 4. Show Notification via OS-specific target
    let system_target = app_handle.state::<super::SystemTarget>();
    use super::target::CaptureTarget;
    let _ = system_target.report_success(&req);

    Ok(())
}

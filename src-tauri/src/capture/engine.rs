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

    // 2. Insert into Database
    let state = app_handle.state::<AppState>();
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    
    let item = operations::create_item(
        &conn,
        None, // Inbox
        &req.data_type,
        &req.content,
        title_opt,
        &metadata,
    ).map_err(|e| e.to_string())?;

    // --- Collect Context Signals ---
    crate::capture::context::collect_and_store_signals(&conn, &req, &item);

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

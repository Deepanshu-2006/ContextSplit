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

    // Drop the mutex guard before emitting events or showing notifications to prevent deadlocks
    drop(conn);

    // 3. Emit Tauri Event
    let _ = app_handle.emit("clipboard://new-item", &item);

    // 4. Show Notification
    let (emoji, action) = match req.data_type.as_str() {
        "url" => ("🔗", "Link grabbed"),
        "image" => ("🖼️", "Image saved"),
        "file" => ("📄", "File saved"),
        _ => ("📝", "Text saved"),
    };

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!("display notification \"Ready in your Inbox\" with title \"ContextSplit\" subtitle \"{} {}\"", action, emoji))
            .status(); 
    }
    
    #[cfg(target_os = "windows")]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app_handle.notification()
            .title("ContextSplit")
            .body(format!("{} {}", action, emoji))
            .show();
    }

    Ok(())
}

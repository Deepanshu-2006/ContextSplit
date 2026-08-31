mod db;
mod clipboard;
pub mod capture;
pub mod context_engine;

use std::sync::Mutex;
use tauri::{Manager, State};
use db::models::{Workspace, Item, ContextSignal};
use context_engine::engine::{ContextEngine, ContextEvent};
use context_engine::providers::{CaptureContextProvider, UserActionContextProvider, WorkspaceContextProvider, ActiveAppProvider, UrlContextProvider};

pub struct AppState {
    pub db_conn: Mutex<rusqlite::Connection>,
    pub context_engine: Mutex<ContextEngine>,
}

#[tauri::command]
fn create_workspace(name: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::create_workspace(&conn, name).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_workspace(id: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::get_workspace(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::list_workspaces(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_workspace(id: &str, name: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::update_workspace(&conn, id, name).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_workspace(id: &str, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::delete_workspace(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_item(
    workspace_id: Option<&str>,
    item_type: &str,
    content: &str,
    title: Option<&str>,
    metadata: &str,
    state: State<'_, AppState>,
) -> Result<Item, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::create_item(&conn, workspace_id, None, item_type, content, title, metadata).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_item(id: &str, state: State<'_, AppState>) -> Result<Item, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::get_item(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_items(workspace_id: Option<&str>, state: State<'_, AppState>) -> Result<Vec<Item>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::list_items(&conn, workspace_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_item(
    id: &str,
    content: Option<&str>,
    title: Option<&str>,
    metadata: Option<&str>,
    state: State<'_, AppState>,
) -> Result<Item, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::update_item(&conn, id, content, title, metadata).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_item(id: &str, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::delete_item(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn move_item(
    id: &str,
    workspace_id: Option<&str>,
    state: State<'_, AppState>,
) -> Result<Item, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    
    let old_item = db::operations::get_item(&conn, id).map_err(|e| e.to_string())?;
    let source_ws = old_item.workspace_id;
    
    let new_item = db::operations::move_item(&conn, id, workspace_id).map_err(|e| e.to_string())?;
    
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    ce.process_event(&conn, ContextEvent::UserAction {
        item_id: id.to_string(),
        action: "moved_to_workspace".to_string(),
        source_workspace: source_ws,
        destination_workspace: workspace_id.map(|s| s.to_string()),
    });
    
    Ok(new_item)
}

#[tauri::command]
fn create_context_signal(
    workspace_id: Option<&str>,
    item_id: Option<&str>,
    signal_type: &str,
    session_id: Option<&str>,
    payload: &str,
    state: State<'_, AppState>,
) -> Result<ContextSignal, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::operations::create_context_signal(&conn, workspace_id, item_id, signal_type, session_id, payload).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_context_signal(id: &str, state: State<'_, AppState>) -> Result<ContextSignal, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::operations::get_context_signal(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_context_signals(
    state: State<'_, AppState>,
    workspace_id: Option<&str>,
    item_id: Option<&str>,
) -> Result<Vec<ContextSignal>, String> {
    let conn = state.db_conn.lock().unwrap();
    context_engine::operations::list_context_signals(&conn, workspace_id, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn score_item(
    state: State<'_, AppState>,
    item_id: &str,
) -> Result<Vec<crate::context_engine::scoring::WorkspaceScore>, String> {
    let conn = state.db_conn.lock().unwrap();
    crate::context_engine::scoring::score_item(&conn, item_id)
}

#[tauri::command]
fn get_strongest_relationships(
    item_id: &str,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::ItemRelationship>, String> {
    let conn = state.db_conn.lock().unwrap();
    crate::context_engine::graph::get_strongest_relationships(&conn, item_id, limit).map_err(|e| e.to_string())
}

#[tauri::command]
fn handle_capture(req: capture::target::CaptureRequest, app: tauri::AppHandle) -> Result<(), String> {
    capture::engine::process_capture(app, req)
}

#[tauri::command]
fn show_notch(state: State<'_, capture::SystemTarget>) -> Result<(), String> {
    use capture::target::CaptureTarget;
    state.show()
}

#[tauri::command]
fn hide_notch(state: State<'_, capture::SystemTarget>) -> Result<(), String> {
    use capture::target::CaptureTarget;
    state.hide()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let conn = db::init(app_data_dir)?;
            
            let mut ce = ContextEngine::new();
            ce.register(Box::new(CaptureContextProvider));
            ce.register(Box::new(UserActionContextProvider));
            ce.register(Box::new(WorkspaceContextProvider));
            ce.register(Box::new(ActiveAppProvider));
            ce.register(Box::new(UrlContextProvider));
            ce.initialize_all().map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error>)?;
            
            app.manage(AppState {
                db_conn: Mutex::new(conn),
                context_engine: Mutex::new(ce),
            });
            
            // Start the clipboard monitor loop
            let app_handle = app.handle().clone();
            clipboard::start_monitor(app_handle.clone());
            
            // Initialize Capture Target Overlay
            let system_target = capture::SystemTarget::new(app_handle.clone());
            app.manage(system_target);
            
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            create_workspace,
            get_workspace,
            list_workspaces,
            update_workspace,
            delete_workspace,
            create_item,
            get_item,
            list_items,
            update_item,
            delete_item,
            move_item,
            handle_capture,
            show_notch,
            hide_notch,
            create_context_signal,
            get_context_signal,
            list_context_signals,
            score_item,
            get_strongest_relationships
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

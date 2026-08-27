mod db;
mod clipboard;

use std::sync::Mutex;
use tauri::{Manager, State};
use db::models::{Workspace, Item};

pub struct AppState {
    pub db_conn: Mutex<rusqlite::Connection>,
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
    db::operations::create_item(&conn, workspace_id, item_type, content, title, metadata).map_err(|e| e.to_string())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let conn = db::init(app_data_dir)?;
            
            app.manage(AppState {
                db_conn: Mutex::new(conn),
            });
            
            // Start the clipboard monitor loop
            let app_handle = app.handle().clone();
            clipboard::start_monitor(app_handle);
            
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
            delete_item
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

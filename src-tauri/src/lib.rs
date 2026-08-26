mod db;

use std::sync::Mutex;
use tauri::{Manager, State};
use db::models::Workspace;

struct AppState {
    db_conn: Mutex<rusqlite::Connection>,
}

#[tauri::command]
fn create_workspace(name: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().unwrap();
    db::operations::create_workspace(&conn, name).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_workspace(id: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().unwrap();
    db::operations::get_workspace(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>, String> {
    let conn = state.db_conn.lock().unwrap();
    db::operations::list_workspaces(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_workspace(id: &str, name: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().unwrap();
    db::operations::update_workspace(&conn, id, name).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_workspace(id: &str, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db_conn.lock().unwrap();
    db::operations::delete_workspace(&conn, id).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            let conn = db::init(app_data_dir).expect("Failed to initialize database");
            
            app.manage(AppState {
                db_conn: Mutex::new(conn),
            });
            
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            create_workspace,
            get_workspace,
            list_workspaces,
            update_workspace,
            delete_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

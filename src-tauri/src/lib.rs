mod db;
mod clipboard;
pub mod capture;
pub mod context_engine;
pub mod semantic;

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
fn get_item_semantic_status(id: &str, state: State<'_, AppState>) -> Result<Option<db::models::ItemSemanticStatus>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::get_item_semantic_status(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_item_embedding_metadata(id: &str, state: State<'_, AppState>) -> Result<Option<db::models::ItemEmbeddingMetadata>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::get_item_embedding_metadata(&conn, id).map_err(|e| e.to_string())
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
fn get_settings(state: State<'_, AppState>) -> Result<crate::db::settings::PrivacySettings, String> {
    let conn = state.db_conn.lock().unwrap();
    crate::db::settings::get_settings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_settings(settings: crate::db::settings::PrivacySettings, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db_conn.lock().unwrap();
    crate::db::settings::update_settings(&conn, settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_semantic_data(state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db_conn.lock().unwrap();
    crate::db::operations::delete_semantic_data(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn rebuild_semantic_data(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db_conn.lock().unwrap();
    crate::db::operations::rebuild_semantic_data(&conn).map_err(|e| e.to_string())?;
    
    // Spawn background retry loop for all the newly pending items
    let app_handle_clone = app_handle.clone();
    std::thread::spawn(move || {
        if let Some(state) = app_handle_clone.try_state::<AppState>() {
            let mut items_to_retry = Vec::new();
            if let Ok(conn) = state.db_conn.lock() {
                if let Ok(settings) = crate::db::settings::get_settings(&conn) {
                    if settings.enable_semantic_processing {
                        if let Ok(pending_items) = crate::db::operations::get_pending_or_failed_items(&conn, 3) {
                            items_to_retry = pending_items;
                        }
                    }
                }
            }
            
            for item_id in items_to_retry {
                crate::capture::engine::retry_item_embedding(app_handle_clone.clone(), &item_id);
            }
        }
    });
    
    Ok(())
}

// ---------------------------------------------------------------------------
// Context Discovery commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn run_context_discovery(
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::ContextCandidate>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::run_discovery(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_context_candidates(
    status_filter: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::ContextCandidate>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::list_candidates(&conn, status_filter.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_context_candidate(
    id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextCandidate, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::get_candidate(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_candidate_items(
    candidate_id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::CandidateWithItems, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::get_candidate_items(&conn, candidate_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_candidate_item(
    candidate_id: &str,
    item_id: &str,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::remove_item_from_candidate(&conn, candidate_id, item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_candidate_status(
    id: &str,
    status: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextCandidate, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::update_candidate_status(&conn, id, status)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn expire_stale_candidates(
    max_age_secs: Option<i64>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let secs = max_age_secs.unwrap_or(72 * 3600);
    crate::context_engine::discovery::expire_stale_candidates(&conn, secs)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_candidate_label(
    id: &str,
    label: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextCandidate, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::update_candidate_label(&conn, id, label)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn generate_candidate_name(
    item_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::generate_candidate_name(&conn, &item_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn score_context_candidate(
    candidate_id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::CandidateScore, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::score_context_candidate(&conn, candidate_id, None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn score_candidate_group(
    item_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<crate::db::models::CandidateScore, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::score_candidate_group(&conn, &item_ids, None)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------

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
            
            // Initialize FastEmbedProvider asynchronously to avoid blocking startup
            let app_handle_for_embed = app_handle.clone();
            std::thread::spawn(move || {
                if let Ok(provider) = crate::semantic::fastembed_provider::FastEmbedProvider::try_new() {
                    if let Some(state) = app_handle_for_embed.try_state::<AppState>() {
                        if let Ok(mut ce) = state.context_engine.lock() {
                            ce.set_semantic_provider(Box::new(provider));
                        }
                        
                        // Retry failed embeddings on startup (moved outside the lock!)
                        let mut items_to_retry = Vec::new();
                        if let Ok(conn) = state.db_conn.lock() {
                            if let Ok(failed_items) = crate::db::operations::get_pending_or_failed_items(&conn, 3) {
                                items_to_retry = failed_items;
                            }
                        }
                        
                        for item_id in items_to_retry {
                            crate::capture::engine::retry_item_embedding(app_handle_for_embed.clone(), &item_id);
                        }
                    }
                }
            });
            
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
            get_strongest_relationships,
            get_item_semantic_status,
            get_item_embedding_metadata,
            get_settings,
            update_settings,
            delete_semantic_data,
            rebuild_semantic_data,
            // Context Discovery
            run_context_discovery,
            list_context_candidates,
            get_context_candidate,
            get_candidate_items,
            remove_candidate_item,
            update_candidate_status,
            update_candidate_label,
            generate_candidate_name,
            expire_stale_candidates,
            score_context_candidate,
            score_candidate_group
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

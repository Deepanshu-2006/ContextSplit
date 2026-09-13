mod db;
mod clipboard;
mod tray;
pub mod capture;
pub mod context_engine;
pub mod search;
pub mod semantic;

use std::sync::Mutex;
use tauri::{Emitter, Manager, State};
use db::models::{Workspace, Item, ContextSignal};
use context_engine::engine::{ContextEngine, ContextEvent};
use context_engine::providers::{CaptureContextProvider, UserActionContextProvider, WorkspaceContextProvider, ActiveAppProvider, UrlContextProvider};

pub struct AppState {
    pub db_conn: Mutex<rusqlite::Connection>,
    pub context_engine: Mutex<ContextEngine>,
    pub capture_paused: std::sync::atomic::AtomicBool,
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
fn update_workspace_lifecycle_state(
    id: &str,
    state: &str,
    app_state: State<'_, AppState>,
) -> Result<Workspace, String> {
    let conn = app_state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::update_workspace_lifecycle_state(&conn, id, state)
}

#[tauri::command]
fn archive_workspace(id: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::archive_workspace(&conn, id)
}

#[tauri::command]
fn unarchive_workspace(id: &str, state: State<'_, AppState>) -> Result<Workspace, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::unarchive_workspace(&conn, id)
}

#[tauri::command]
fn evaluate_workspace_lifecycle(
    id: &str,
    inactivity_days_threshold: Option<f64>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::evaluate_workspace_lifecycle(&conn, id, inactivity_days_threshold)
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
fn empty_inbox(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<usize, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let count = db::operations::empty_inbox(&conn).map_err(|e| e.to_string())?;
    let _ = app.emit("inbox://updated", ());
    Ok(count)
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
fn evaluate_item_context(
    state: State<'_, AppState>,
    item_id: &str,
) -> Result<crate::db::models::UnifiedContextInterpretation, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::scoring::evaluate_item_context(&conn, item_id)
}

#[tauri::command]
fn debug_item_context_confidence(
    state: State<'_, AppState>,
    item_id: &str,
) -> Result<crate::context_engine::debugger::ContextConfidenceDebugTrace, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::debugger::debug_item_context_confidence(&conn, item_id)
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

pub fn set_capture_paused_internal(app_handle: &tauri::AppHandle, paused: bool) -> Result<bool, String> {
    if let Some(state) = app_handle.try_state::<AppState>() {
        state.capture_paused.store(paused, std::sync::atomic::Ordering::Relaxed);
        if let Ok(conn) = state.db_conn.lock() {
            if let Ok(mut settings) = crate::db::settings::get_settings(&conn) {
                settings.capture_paused = paused;
                let _ = crate::db::settings::update_settings(&conn, settings);
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if let Some(target) = app_handle.try_state::<crate::capture::SystemTarget>() {
        #[cfg(target_os = "macos")]
        target.notch_controller.set_paused(paused);
        #[cfg(target_os = "windows")]
        target.windows_controller.set_paused(paused);
    }

    crate::tray::update_tray_capture_status(paused);
    let _ = app_handle.emit("capture-pause-changed", paused);

    Ok(paused)
}

#[tauri::command]
fn toggle_capture_pause(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<bool, String> {
    let current = state.capture_paused.load(std::sync::atomic::Ordering::Relaxed);
    set_capture_paused_internal(&app, !current)
}

#[tauri::command]
fn set_capture_paused(paused: bool, app: tauri::AppHandle) -> Result<bool, String> {
    set_capture_paused_internal(&app, paused)
}

#[tauri::command]
fn is_capture_paused(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.capture_paused.load(std::sync::atomic::Ordering::Relaxed))
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<crate::db::settings::PrivacySettings, String> {
    let conn = state.db_conn.lock().unwrap();
    crate::db::settings::get_settings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_settings(settings: crate::db::settings::PrivacySettings, state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let paused = settings.capture_paused;
    state.capture_paused.store(paused, std::sync::atomic::Ordering::Relaxed);

    let conn = state.db_conn.lock().unwrap();
    crate::db::settings::update_settings(&conn, settings).map_err(|e| e.to_string())?;

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if let Some(target) = app.try_state::<crate::capture::SystemTarget>() {
        #[cfg(target_os = "macos")]
        target.notch_controller.set_paused(paused);
        #[cfg(target_os = "windows")]
        target.windows_controller.set_paused(paused);
    }

    crate::tray::update_tray_capture_status(paused);
    let _ = app.emit("capture-pause-changed", paused);

    Ok(())
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
fn create_workspace_from_candidate(
    candidate_id: &str,
    custom_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<crate::db::models::Workspace, String> {
    let mut conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::discovery::accept_candidate_and_create_workspace(
        &mut conn,
        candidate_id,
        custom_name.as_deref(),
    )
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
// Context Membership Commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn create_context_membership(
    item_id: &str,
    workspace_id: &str,
    membership_type: &str,
    confidence: Option<f64>,
    source: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextMembership, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::create_or_update_membership(
        &conn,
        item_id,
        workspace_id,
        membership_type,
        confidence.unwrap_or(1.0),
        source,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_context_membership(
    id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextMembership, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::get_membership(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_item_workspace_context_membership(
    item_id: &str,
    workspace_id: &str,
    state: State<'_, AppState>,
) -> Result<Option<crate::db::models::ContextMembership>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::get_item_workspace_membership(&conn, item_id, workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_item_context_memberships(
    item_id: &str,
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::ContextMembership>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::list_item_memberships(&conn, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_workspace_context_memberships(
    workspace_id: &str,
    membership_type: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::ContextMembership>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::list_workspace_memberships(&conn, workspace_id, membership_type.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn confirm_context_membership(
    item_id: &str,
    workspace_id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextMembership, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::confirm_membership(&conn, item_id, workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn dismiss_context_membership(
    item_id: &str,
    workspace_id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextMembership, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::dismiss_membership(&conn, item_id, workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_context_membership(
    id: &str,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::delete_membership(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn recalculate_workspace_suggested_memberships(
    workspace_id: &str,
    trigger_item_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::ContextMembership>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::scoring::recalculate_workspace_suggested_memberships(
        &conn,
        workspace_id,
        trigger_item_id.as_deref(),
    )
}

#[tauri::command]
fn bulk_assign_inbox_items_to_workspace(
    item_ids: Vec<String>,
    workspace_id: &str,
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::Item>, String> {
    let mut conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::bulk_assign_inbox_items_to_workspace(&mut conn, &item_ids, workspace_id)
}

#[tauri::command]
fn bulk_dismiss_inbox_suggestions(
    item_ids: Vec<String>,
    workspace_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::bulk_dismiss_inbox_suggestions(&mut conn, &item_ids, workspace_id.as_deref())
}

#[tauri::command]
fn bulk_archive_or_clear_inbox_items(
    item_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let mut conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::db::operations::bulk_archive_or_clear_inbox_items(&mut conn, &item_ids)
}

// ---------------------------------------------------------------------------
// Context Timeline commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_context_timeline(
    workspace_id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextTimeline, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::timeline::get_context_timeline(&conn, workspace_id)
}

// ---------------------------------------------------------------------------
// Context Explanation commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn explain_context_association(
    item_id: &str,
    workspace_id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextExplanation, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::explanation::explain_context_association(&conn, item_id, workspace_id)
}

// ---------------------------------------------------------------------------
// Context Overview command
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_context_overview(
    workspace_id: &str,
    state: State<'_, AppState>,
) -> Result<crate::db::models::ContextOverview, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::overview::get_context_overview(&conn, workspace_id)
}

// ---------------------------------------------------------------------------
// Temporal Context Weighting commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_temporal_decay_config(
    state: State<'_, AppState>,
) -> Result<crate::context_engine::temporal::TemporalDecayConfig, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    Ok(crate::context_engine::temporal::get_temporal_decay_config(&conn))
}

#[tauri::command]
fn update_temporal_decay_config(
    config: crate::context_engine::temporal::TemporalDecayConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    crate::context_engine::temporal::update_temporal_decay_config(&conn, &config)
}

// ---------------------------------------------------------------------------
// AI Context Reasoning commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn analyze_item_context_ai(
    item_id: &str,
    is_user_explicit: bool,
    state: State<'_, AppState>,
) -> Result<context_engine::reasoning::AIContextReasoningResponse, String> {
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    // Use the graceful variant — AI failures are handled internally and never propagate
    // as hard errors. Deterministic context features are unaffected.
    let response = context_engine::reasoning::analyze_item_context_ai_graceful(
        &conn,
        item_id,
        ce.ai_reasoner().as_ref(),
        is_user_explicit,
    );
    Ok(response)
}

#[tauri::command]
fn analyze_candidate_context_ai(
    candidate_id: &str,
    is_user_explicit: bool,
    state: State<'_, AppState>,
) -> Result<context_engine::reasoning::AIContextReasoningResponse, String> {
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let response = context_engine::reasoning::analyze_candidate_context_ai_graceful(
        &conn,
        candidate_id,
        ce.ai_reasoner().as_ref(),
        is_user_explicit,
    );
    Ok(response)
}

#[tauri::command]
fn analyze_item_group_context_ai(
    item_ids: Vec<String>,
    is_user_explicit: bool,
    state: State<'_, AppState>,
) -> Result<context_engine::reasoning::AIContextReasoningResponse, String> {
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let response = context_engine::reasoning::analyze_item_group_context_ai_graceful(
        &conn,
        &item_ids,
        ce.ai_reasoner().as_ref(),
        is_user_explicit,
    );
    Ok(response)
}

#[tauri::command]
fn get_evidence_package(
    item_id: &str,
    state: State<'_, AppState>,
) -> Result<context_engine::reasoning::EvidencePackage, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::reasoning::build_evidence_package(&conn, item_id)
}

#[tauri::command]
fn get_context_evidence_package(
    item_id: &str,
    state: State<'_, AppState>,
) -> Result<context_engine::reasoning::ContextEvidencePackage, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::reasoning::build_context_evidence_package(&conn, item_id)
}

#[tauri::command]
fn get_candidate_evidence_package(
    candidate_id: &str,
    state: State<'_, AppState>,
) -> Result<context_engine::reasoning::EvidencePackage, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::reasoning::build_candidate_evidence_package(&conn, candidate_id)
}

#[tauri::command]
fn suggest_candidate_name_ai(
    candidate_id: &str,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    ce.suggest_candidate_name_ai(&conn, candidate_id)
}

#[tauri::command]
fn suggest_item_group_name_ai(
    item_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    ce.suggest_item_group_name_ai(&conn, &item_ids)
}

#[tauri::command]
fn get_ai_provider_status(
    state: State<'_, AppState>,
) -> Result<context_engine::reasoning::AIProviderStatus, String> {
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    let reasoner = ce.ai_reasoner();
    Ok(context_engine::reasoning::get_reasoner_status(reasoner.as_ref()))
}

#[tauri::command]
fn clear_ai_reasoning_cache(
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::reasoning::cache::clear_reasoning_cache(&conn, None)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Day 12: Context-Aware Inbox State Machine commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_inbox_item_state(
    item_id: &str,
    state: State<'_, AppState>,
) -> Result<Option<db::models::InboxItemState>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    db::operations::get_inbox_item_state(&conn, item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn transition_inbox_item_state(
    item_id: &str,
    target_state: &str,
    reason: Option<&str>,
    state: State<'_, AppState>,
) -> Result<db::models::InboxItemState, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let parsed_state = db::models::InboxState::from_str(target_state)?;
    db::operations::transition_inbox_item_state(&conn, item_id, parsed_state, reason)
}

#[tauri::command]
fn list_inbox_item_states(
    state_filter: Option<&str>,
    state: State<'_, AppState>,
) -> Result<Vec<db::models::InboxItemState>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let filter = match state_filter {
        Some(s) if !s.is_empty() => Some(db::models::InboxState::from_str(s)?),
        _ => None,
    };
    db::operations::list_inbox_item_states(&conn, filter).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Day 12: Inbox Attention Engine commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn evaluate_inbox_item_attention(
    item_id: &str,
    config: Option<context_engine::attention::AttentionConfig>,
    state: State<'_, AppState>,
) -> Result<context_engine::attention::ItemAttention, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::attention::evaluate_inbox_item_attention(&conn, item_id, config.as_ref())
}

#[tauri::command]
fn evaluate_inbox_attention_batch(
    config: Option<context_engine::attention::AttentionConfig>,
    state: State<'_, AppState>,
) -> Result<Vec<context_engine::attention::ItemAttention>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    context_engine::attention::evaluate_inbox_attention_batch(&conn, config.as_ref())
}

// ---------------------------------------------------------------------------
// Inbox Search Architecture
// ---------------------------------------------------------------------------

#[tauri::command]
fn search_inbox_items(
    params: search::InboxSearchParams,
    state: State<'_, AppState>,
) -> Result<Vec<search::InboxSearchResult>, String> {
    let ce = state.context_engine.lock().map_err(|_| "Context engine lock poisoned".to_string())?;
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let semantic_provider = ce.get_semantic_provider();
    let provider_ref = semantic_provider.as_ref().map(|p| p.as_ref().as_ref());
    search::search_inbox_items(&conn, &params, provider_ref).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------

#[tauri::command]
fn capture_raw_event(payload: capture::RawCapturePayload, app: tauri::AppHandle) -> Result<capture::CaptureResult, String> {
    let core = capture::CaptureCore::new();
    core.process_app(app, payload).map_err(|e| e.to_string())
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

#[tauri::command]
fn notch_drag_enter(state: State<'_, capture::SystemTarget>) {
    #[cfg(target_os = "macos")]
    state.notch_controller.on_drag_enter();
}

#[tauri::command]
fn notch_drag_over(is_near_center: bool, state: State<'_, capture::SystemTarget>) {
    #[cfg(target_os = "macos")]
    state.notch_controller.on_drag_over(is_near_center);
}

#[tauri::command]
fn notch_drag_leave(state: State<'_, capture::SystemTarget>) {
    #[cfg(target_os = "macos")]
    state.notch_controller.on_drag_leave();
}

#[tauri::command]
fn notch_drop(state: State<'_, capture::SystemTarget>) -> bool {
    #[cfg(target_os = "macos")]
    return state.notch_controller.on_drop();
    
    #[cfg(not(target_os = "macos"))]
    return false;
}

#[tauri::command]
fn show_windows_target(state: State<'_, capture::SystemTarget>) -> Result<(), String> {
    use capture::target::CaptureTarget;
    state.show()
}

#[tauri::command]
fn hide_windows_target(state: State<'_, capture::SystemTarget>) -> Result<(), String> {
    use capture::target::CaptureTarget;
    state.hide()
}

#[tauri::command]
fn windows_target_drag_enter(_state: State<'_, capture::SystemTarget>) {
    #[cfg(target_os = "windows")]
    _state.windows_controller.on_drag_enter();
}

#[tauri::command]
fn windows_target_drag_over(_is_inside_target: bool, _state: State<'_, capture::SystemTarget>) {
    #[cfg(target_os = "windows")]
    _state.windows_controller.on_drag_over(_is_inside_target);
}

#[tauri::command]
fn windows_target_drag_leave(_state: State<'_, capture::SystemTarget>) {
    #[cfg(target_os = "windows")]
    _state.windows_controller.on_drag_leave();
}

#[tauri::command]
fn windows_target_drop(_state: State<'_, capture::SystemTarget>) -> bool {
    #[cfg(target_os = "windows")]
    return _state.windows_controller.on_drop();
    
    #[cfg(not(target_os = "windows"))]
    return false;
}

// =========================================================================
// Context Memory Queries (Day 17)
// =========================================================================

#[tauri::command]
fn get_current_memory(
    state: State<'_, AppState>,
) -> Result<Option<context_engine::memory_query::ContextMemoryResponse>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_current_memory(&conn)
}

#[tauri::command]
fn get_context_memory(
    context_id: &str,
    state: State<'_, AppState>,
) -> Result<Option<context_engine::memory_query::ContextMemoryResponse>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_context_memory(&conn, context_id)
}

#[tauri::command]
fn get_recent_context_memories(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<context_engine::memory_query::ContextMemoryResponse>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_recent_context_memories(&conn, limit)
}

#[tauri::command]
fn get_last_active_context(
    state: State<'_, AppState>,
) -> Result<Option<context_engine::memory_query::ContextMemoryResponse>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_last_active_context(&conn)
}

#[tauri::command]
fn get_last_active_task(
    context_id: &str,
    state: State<'_, AppState>,
) -> Result<Option<context_engine::memory_query::LastActiveTaskResponse>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_last_active_task(&conn, context_id)
}

#[tauri::command]
fn get_recent_work_sequence(
    limit_days: Option<usize>,
    state: State<'_, AppState>,
) -> Result<context_engine::work_sequence::RecentWorkSequenceResponse, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let service = context_engine::work_sequence::RecentWorkSequenceService::new();
    service.get_recent_work_sequence(&conn, limit_days)
}

#[tauri::command]
fn get_today_memory(
    state: State<'_, AppState>,
) -> Result<context_engine::memory_query::HistoricalContextMemoryResponse, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_today_memory(&conn)
}

#[tauri::command]
fn get_yesterday_memory(
    state: State<'_, AppState>,
) -> Result<context_engine::memory_query::HistoricalContextMemoryResponse, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_yesterday_memory(&conn)
}

#[tauri::command]
fn get_historical_memory_for_date(
    date: String,
    state: State<'_, AppState>,
) -> Result<context_engine::memory_query::HistoricalContextMemoryResponse, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_historical_memory_for_date(&conn, &date)
}

#[tauri::command]
fn get_recent_historical_memories(
    limit_days: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<context_engine::memory_query::HistoricalContextMemoryResponse>, String> {
    let conn = state.db_conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
    let query_service = context_engine::memory_query::ContextMemoryQueryService::new();
    query_service.get_recent_historical_memories(&conn, limit_days)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.show();
                let _ = main_win.set_focus();
            }

            tray::create_tray(app)?;

            let app_data_dir = app.path().app_data_dir().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let conn = db::init(app_data_dir)?;
            
            let initial_settings = crate::db::settings::get_settings(&conn).unwrap_or_default();
            let initial_paused = initial_settings.capture_paused;

            let mut ce = ContextEngine::from_env();
            ce.register(Box::new(CaptureContextProvider));
            ce.register(Box::new(UserActionContextProvider));
            ce.register(Box::new(WorkspaceContextProvider));
            ce.register(Box::new(ActiveAppProvider));
            ce.register(Box::new(UrlContextProvider));
            ce.initialize_all().map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error>)?;
            
            app.manage(AppState {
                db_conn: Mutex::new(conn),
                context_engine: Mutex::new(ce),
                capture_paused: std::sync::atomic::AtomicBool::new(initial_paused),
            });
            
            crate::tray::update_tray_capture_status(initial_paused);

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
            #[cfg(target_os = "macos")]
            system_target.notch_controller.set_paused(initial_paused);
            #[cfg(target_os = "windows")]
            system_target.windows_controller.set_paused(initial_paused);
            app.manage(system_target);
            
            // Initialize Capture Toast Window
            if app_handle.get_webview_window("capture-toast").is_none() {
                let _ = tauri::WebviewWindowBuilder::new(
                    &app_handle,
                    "capture-toast",
                    tauri::WebviewUrl::App("index.html".into())
                )
                .title("Capture Notification")
                .inner_size(320.0, 100.0)
                .transparent(true)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .accept_first_mouse(false)
                .focused(false)
                .build()
                .map(|window| {
                    if let Ok(Some(monitor)) = window.primary_monitor() {
                        let monitor_size = monitor.size();
                        let window_size = window.outer_size().unwrap_or(tauri::PhysicalSize::new(320, 100));
                        
                        let x = monitor_size.width.saturating_sub(window_size.width + 24) as i32;
                        let y = monitor_size.height.saturating_sub(window_size.height + 24) as i32;
                        
                        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                    }
                });
            }
            
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
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
            empty_inbox,
            move_item,
            capture_raw_event,
            handle_capture,
            show_notch,
            hide_notch,
            notch_drag_enter,
            notch_drag_over,
            notch_drag_leave,
            notch_drop,
            show_windows_target,
            hide_windows_target,
            windows_target_drag_enter,
            windows_target_drag_over,
            windows_target_drag_leave,
            windows_target_drop,
            toggle_capture_pause,
            set_capture_paused,
            is_capture_paused,
            create_context_signal,
            get_context_signal,
            list_context_signals,
            score_item,
            evaluate_item_context,
            debug_item_context_confidence,
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
            create_workspace_from_candidate,
            generate_candidate_name,
            expire_stale_candidates,
            score_context_candidate,
            score_candidate_group,
            // Context Membership
            create_context_membership,
            get_context_membership,
            get_item_workspace_context_membership,
            list_item_context_memberships,
            list_workspace_context_memberships,
            confirm_context_membership,
            dismiss_context_membership,
            delete_context_membership,
            recalculate_workspace_suggested_memberships,
            // Context Timeline
            get_context_timeline,
            // Context Explanation
            explain_context_association,
            // Temporal Context Weighting
            get_temporal_decay_config,
            update_temporal_decay_config,
            // Workspace Lifecycle
            update_workspace_lifecycle_state,
            archive_workspace,
            unarchive_workspace,
            evaluate_workspace_lifecycle,
            // Context Overview
            get_context_overview,
            // AI Context Reasoning
            analyze_item_context_ai,
            analyze_candidate_context_ai,
            analyze_item_group_context_ai,
            get_evidence_package,
            get_context_evidence_package,
            get_candidate_evidence_package,
            suggest_candidate_name_ai,
            suggest_item_group_name_ai,
            get_ai_provider_status,
            clear_ai_reasoning_cache,
            // Context-Aware Inbox State Machine (Day 12)
            get_inbox_item_state,
            transition_inbox_item_state,
            list_inbox_item_states,
            // Inbox Attention Engine
            evaluate_inbox_item_attention,
            evaluate_inbox_attention_batch,
            // Multi-Select Bulk Inbox Operations
            bulk_assign_inbox_items_to_workspace,
            bulk_dismiss_inbox_suggestions,
            bulk_archive_or_clear_inbox_items,
            // Inbox Search Architecture
            search_inbox_items,
            // Context Memory Queries (Day 17)
            get_current_memory,
            get_context_memory,
            get_recent_context_memories,
            get_last_active_context,
            get_last_active_task,
            get_recent_work_sequence,
            get_today_memory,
            get_yesterday_memory,
            get_historical_memory_for_date,
            get_recent_historical_memories
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

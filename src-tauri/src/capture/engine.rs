use tauri::{AppHandle, Manager};

use crate::AppState;
use super::core::CaptureCore;
use super::models::{CaptureMethod, RawCapturePayload};
use super::target::CaptureRequest;

/// Legacy entry point for processing a capture request from the UI, now routing directly through CaptureCore.
pub fn process_capture(app_handle: AppHandle, req: CaptureRequest) -> Result<(), String> {
    let source_method = req
        .source
        .as_deref()
        .map(CaptureMethod::from)
        .unwrap_or(CaptureMethod::Manual);

    let mut payload = RawCapturePayload::new(req.content.clone(), source_method);
    payload.data_type = Some(req.data_type.clone());
    payload.title = req.title.clone();
    payload.mime_type = req.mime_type.clone();
    payload.original_url = req.original_url.clone();
    payload.size_bytes = req.size;
    payload.workspace_id = req.workspace_id.clone();
    if let Some(_f) = &req.filename {
        if !req.content.starts_with("data:") {
            payload.file_paths = vec![req.content.clone()];
        }
    }

    let core = CaptureCore::new();
    let _result = core.process_app(app_handle.clone(), payload).map_err(|e| e.to_string())?;

    // Report success to target overlay if active, but ONLY for non-drag-and-drop sources (like hotkeys).
    // For drag-and-drop, the UI handles the success animation instantly on the window drop event,
    // so a delayed report_success from the backend would cause a duplicate ghost animation.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if req.source.as_deref() != Some("drag_drop") {
            if let Some(system_target) = app_handle.try_state::<super::SystemTarget>() {
                use super::target::CaptureTarget;
                let _ = system_target.report_success(&req);
            }
        }
    }

    Ok(())
}

/// Retries failed semantic embedding generation for a captured item.
pub fn retry_item_embedding(app_handle: AppHandle, item_id: &str) {
    let state = app_handle.state::<AppState>();

    // Fetch item with a brief lock
    let item_opt = {
        if let Ok(conn) = state.db_conn.lock() {
            crate::db::operations::get_item(&conn, item_id).ok()
        } else {
            None
        }
    };

    let item = match item_opt {
        Some(i) => i,
        None => return,
    };

    // Check settings with a brief lock
    let enable_semantic = {
        if let Ok(conn) = state.db_conn.lock() {
            crate::db::settings::get_settings(&conn)
                .map(|s| s.enable_semantic_processing)
                .unwrap_or(true)
        } else {
            true
        }
    };

    if !enable_semantic {
        return;
    }

    // Get provider with a brief lock
    let provider_opt = {
        if let Ok(ce) = state.context_engine.lock() {
            ce.get_semantic_provider()
        } else {
            None
        }
    };

    let provider = match provider_opt {
        Some(p) => p,
        None => return,
    };

    let builder = crate::semantic::builder::SemanticContentBuilder::new();
    let text_to_embed = builder.build(&item);

    if text_to_embed.trim().is_empty() {
        return;
    }

    let item_id_clone = item_id.to_string();
    let app_handle_clone = app_handle.clone();

    // Mark as processing with a brief lock
    if let Ok(conn) = state.db_conn.lock() {
        let _ = crate::db::operations::update_item_semantic_status(
            &conn,
            &item_id_clone,
            "processing",
            None,
        );
    }

    tauri::async_runtime::spawn(async move {
        let join_handle = tauri::async_runtime::spawn_blocking(
            move || -> Result<(Vec<f32>, String), String> {
                match provider.generate_embedding(&text_to_embed) {
                    Ok(emb) => Ok((emb, provider.status().model_name.clone())),
                    Err(e) => Err(e),
                }
            },
        );

        let mut success = false;
        let mut final_model = None;

        if let Ok(result) = join_handle.await {
            if let Ok((embedding, model_name)) = result {
                final_model = Some(model_name.clone());
                if let Some(state) = app_handle_clone.try_state::<crate::AppState>() {
                    // Save embedding
                    let saved = if let Ok(conn) = state.db_conn.lock() {
                        crate::db::operations::save_item_embedding(
                            &conn,
                            &item_id_clone,
                            &embedding,
                            &model_name,
                            &model_name,
                        )
                        .is_ok()
                    } else {
                        false
                    };

                    if saved {
                        // Generate relationships in a fresh lock pair
                        if let Ok(ce) = state.context_engine.lock() {
                            if let Ok(conn) = state.db_conn.lock() {
                                let _ = crate::context_engine::relationships::generate_semantic_relationships(
                                    &ce,
                                    &conn,
                                    &item_id_clone,
                                );
                            }
                        }
                        success = true;
                    }
                }
            }
        }

        if let Some(state) = app_handle_clone.try_state::<crate::AppState>() {
            if let Ok(conn) = state.db_conn.lock() {
                if success {
                    let _ = crate::db::operations::update_item_semantic_status(
                        &conn,
                        &item_id_clone,
                        "ready",
                        final_model.as_deref(),
                    );
                } else {
                    let _ = crate::db::operations::update_item_semantic_status(
                        &conn,
                        &item_id_clone,
                        "failed",
                        None,
                    );
                    let _ = crate::db::operations::increment_semantic_retries(
                        &conn,
                        &item_id_clone,
                    );
                }
            }
        }
    });
}

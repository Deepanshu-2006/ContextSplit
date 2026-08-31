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
        
        // Generate relationships with recent items
        let _ = crate::context_engine::relationships::generate_relationships(&conn, &item.id);
        
        // --- Asynchronous Semantic Embedding ---
        let enable_semantic = if let Ok(settings) = crate::db::settings::get_settings(&conn) {
            settings.enable_semantic_processing
        } else { true };
        
        if enable_semantic {
            if let Some(provider) = ce.get_semantic_provider() {
            let builder = crate::semantic::builder::SemanticContentBuilder::new();
            let text_to_embed = builder.build(&item);
            
            if !text_to_embed.trim().is_empty() {
                let item_id = item.id.clone();
                let app_handle_clone = app_handle.clone();
                
                // Mark as processing
                let _ = crate::db::operations::update_item_semantic_status(&conn, &item_id, "processing", None);
                
                tauri::async_runtime::spawn(async move {
                    // Offload CPU-intensive embedding generation to a blocking thread
                    let join_handle = tauri::async_runtime::spawn_blocking(move || -> Result<(Vec<f32>, String), String> {
                        match provider.generate_embedding(&text_to_embed) {
                            Ok(emb) => Ok((emb, provider.status().model_name.clone())),
                            Err(e) => Err(e),
                        }
                    });
                    
                    let mut success = false;
                    let mut final_model = None;
                    
                    if let Ok(result) = join_handle.await {
                        if let Ok(tuple_res) = result {
                            let embedding: Vec<f32> = tuple_res.0;
                            let model_name: String = tuple_res.1;
                            final_model = Some(model_name.clone());
                            
                            // After generation, get DB lock briefly to save
                            if let Some(state) = app_handle_clone.try_state::<crate::AppState>() {
                                if let Ok(conn) = state.db_conn.lock() {
                                    if crate::db::operations::save_item_embedding(
                                        &conn,
                                        &item_id,
                                        &embedding,
                                        &model_name,
                                        &model_name
                                    ).is_ok() {
                                        if let Ok(ce) = state.context_engine.lock() {
                                            let _ = crate::context_engine::relationships::generate_semantic_relationships(&ce, &conn, &item_id);
                                        }
                                        success = true;
                                    }
                                }
                            }
                        }
                    }
                    
                    // Final state update
                    if let Some(state) = app_handle_clone.try_state::<crate::AppState>() {
                        if let Ok(conn) = state.db_conn.lock() {
                            if success {
                                let _ = crate::db::operations::update_item_semantic_status(
                                    &conn,
                                    &item_id,
                                    "ready",
                                    final_model.as_deref()
                                );
                            } else {
                                let _ = crate::db::operations::update_item_semantic_status(
                                    &conn,
                                    &item_id,
                                    "failed",
                                    None
                                );
                                let _ = crate::db::operations::increment_semantic_retries(&conn, &item_id);
                            }
                        }
                    }
                });
            } else {
                // If there's no text to embed, mark as ready immediately
                let _ = crate::db::operations::update_item_semantic_status(&conn, &item.id, "ready", None);
            }
        } else {
            // Processing disabled by settings
            let _ = crate::db::operations::update_item_semantic_status(&conn, &item.id, "failed", None);
        }
    } else {
        // Disabled by settings or no provider
        let _ = crate::db::operations::update_item_semantic_status(&conn, &item.id, "failed", None);
    }
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

pub fn retry_item_embedding(app_handle: AppHandle, item_id: &str) {
    let state = app_handle.state::<AppState>();
    
    // Fetch item
    let item_opt = {
        if let Ok(conn) = state.db_conn.lock() {
            if let Ok(i) = crate::db::operations::get_item(&conn, item_id) {
                Some(i)
            } else { None }
        } else { None }
    };
    
    if let Some(item) = item_opt {
        if let Ok(ce) = state.context_engine.lock() {
            let enable_semantic = if let Ok(conn) = state.db_conn.lock() {
                if let Ok(settings) = crate::db::settings::get_settings(&conn) {
                    settings.enable_semantic_processing
                } else { true }
            } else { true };
            
            if enable_semantic {
                if let Some(provider) = ce.get_semantic_provider() {
                let builder = crate::semantic::builder::SemanticContentBuilder::new();
                let text_to_embed = builder.build(&item);
                
                if !text_to_embed.trim().is_empty() {
                    let app_handle_clone = app_handle.clone();
                    let item_id_clone = item_id.to_string();
                    
                    if let Ok(conn) = state.db_conn.lock() {
                        let _ = crate::db::operations::update_item_semantic_status(&conn, &item_id_clone, "processing", None);
                    }
                    
                    tauri::async_runtime::spawn(async move {
                        let join_handle = tauri::async_runtime::spawn_blocking(move || -> Result<(Vec<f32>, String), String> {
                            match provider.generate_embedding(&text_to_embed) {
                                Ok(emb) => Ok((emb, provider.status().model_name.clone())),
                                Err(e) => Err(e),
                            }
                        });
                        
                        let mut success = false;
                        let mut final_model = None;
                        
                        if let Ok(result) = join_handle.await {
                            if let Ok(tuple_res) = result {
                                let embedding = tuple_res.0;
                                let model_name = tuple_res.1;
                                final_model = Some(model_name.clone());
                                
                                if let Some(state) = app_handle_clone.try_state::<crate::AppState>() {
                                    if let Ok(conn) = state.db_conn.lock() {
                                        if crate::db::operations::save_item_embedding(
                                            &conn,
                                            &item_id_clone,
                                            &embedding,
                                            &model_name,
                                            &model_name
                                        ).is_ok() {
                                            if let Ok(ce) = state.context_engine.lock() {
                                                let _ = crate::context_engine::relationships::generate_semantic_relationships(&ce, &conn, &item_id_clone);
                                            }
                                            success = true;
                                        }
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
                                        final_model.as_deref()
                                    );
                                } else {
                                    let _ = crate::db::operations::update_item_semantic_status(
                                        &conn,
                                        &item_id_clone,
                                        "failed",
                                        None
                                    );
                                    let _ = crate::db::operations::increment_semantic_retries(&conn, &item_id_clone);
                                }
                            }
                        }
                    });
                }
            }
        }
    }
}
}

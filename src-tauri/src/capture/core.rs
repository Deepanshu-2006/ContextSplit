use rusqlite::Connection;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use super::models::{CaptureError, CaptureResult, NormalizedCaptureEvent, RawCapturePayload};
use super::normalizer::CaptureNormalizer;
use super::validator::CaptureValidator;
use crate::context_engine::engine::{ContextEngine, ContextEvent};
use crate::db::operations;
use crate::AppState;

/// The central platform-independent capture engine.
#[derive(Debug, Clone, Default)]
pub struct CaptureCore {
    validator: CaptureValidator,
    normalizer: CaptureNormalizer,
}

impl CaptureCore {
    pub fn new() -> Self {
        Self {
            validator: CaptureValidator::new(),
            normalizer: CaptureNormalizer::new(),
        }
    }

    pub fn with_validator(mut self, validator: CaptureValidator) -> Self {
        self.validator = validator;
        self
    }

    /// Step 1: Validate the raw event
    pub fn validate(&self, raw: &RawCapturePayload) -> Result<(), CaptureError> {
        self.validator.validate(raw)
    }

    /// Step 2: Normalize the raw event
    pub fn normalize(&self, raw: &RawCapturePayload) -> Result<NormalizedCaptureEvent, CaptureError> {
        self.normalizer.normalize(raw)
    }

    /// Executes the full 5-step capture pipeline in headless mode (pure Database + ContextEngine).
    /// Perfect for tests and non-GUI headless execution.
    pub fn process_headless(
        &self,
        conn: &Connection,
        context_engine: Option<&ContextEngine>,
        raw: RawCapturePayload,
    ) -> Result<CaptureResult, CaptureError> {
        // Step 1: Validate the event
        self.validate(&raw)?;

        // Step 2: Normalize the content
        let mut event = self.normalize(&raw)?;

        // Step 3: Create / Update the Item in the Database
        let session_id = context_engine.and_then(|ce| ce.get_or_update_active_session(conn).ok());
        event.session_id = session_id.clone();

        let metadata_str = event.metadata.to_string();
        let item = operations::create_item(
            conn,
            event.workspace_id.as_deref(),
            session_id.as_deref(),
            &event.item_type,
            &event.content,
            event.title.as_deref(),
            &metadata_str,
        )
        .map_err(|e| CaptureError::DatabaseError(e.to_string()))?;

        // Read privacy & semantic settings
        let settings = crate::db::settings::get_settings(conn).unwrap_or_default();
        let enable_semantic = settings.enable_semantic_processing;

        let semantic_status = if enable_semantic { "pending" } else { "failed" };
        let _ = operations::update_item_semantic_status(conn, &item.id, semantic_status, None);

        // Step 4: Generate ContextSignals
        let signals_emitted = self.generate_context_signals(conn, &item.id, &event, &settings)?;

        // Step 5: Pass the Item into the Context Engine
        if let Some(ce) = context_engine {
            ce.process_event(
                conn,
                ContextEvent::Capture {
                    item_id: item.id.clone(),
                    workspace_id: event.workspace_id.clone(),
                    source: event.source_method.to_string(),
                    data_type: event.item_type.clone(),
                    metadata: event.metadata.clone(),
                },
            );

            // Generate deterministic graph relationships
            let _ = crate::context_engine::relationships::generate_relationships(conn, &item.id);

            // If a semantic provider is attached in headless mode, trigger embedding synchronously if desired
            if enable_semantic {
                if let Some(provider) = ce.get_semantic_provider() {
                    let builder = crate::semantic::builder::SemanticContentBuilder::new();
                    let text_to_embed = builder.build(&item);
                    if !text_to_embed.trim().is_empty() {
                        if let Ok(emb) = provider.generate_embedding(&text_to_embed) {
                            let model = provider.status().model_name;
                            let _ = operations::save_item_embedding(conn, &item.id, &emb, &model, &model);
                            let _ = operations::update_item_semantic_status(conn, &item.id, "ready", Some(&model));
                            let _ = crate::context_engine::relationships::generate_semantic_relationships(ce, conn, &item.id);
                        }
                    }
                }
            }
        }

        Ok(CaptureResult {
            item,
            signals_emitted,
            session_id,
            semantic_status: semantic_status.to_string(),
            is_duplicate: false,
        })
    }

    /// Step 4 implementation: Generates and stores structured ContextSignal records.
    fn generate_context_signals(
        &self,
        conn: &Connection,
        item_id: &str,
        event: &NormalizedCaptureEvent,
        settings: &crate::db::settings::PrivacySettings,
    ) -> Result<usize, CaptureError> {
        let mut count = 0;

        // 1. Capture Event Signal (Always recorded)
        let capture_payload = json!({
            "source": event.source_method.to_string(),
            "platform": event.platform.to_string(),
            "data_type": event.item_type,
            "size": event.size_bytes,
            "captured_at": event.captured_at
        })
        .to_string();

        if crate::context_engine::operations::create_context_signal(
            conn,
            event.workspace_id.as_deref(),
            Some(item_id),
            "capture_event",
            event.session_id.as_deref(),
            &capture_payload,
        )
        .is_ok()
        {
            count += 1;
        }

        // 2. Active App Signal (If present and permitted by privacy)
        if settings.enable_active_app {
            if let Some(app) = &event.app_name {
                let app_payload = json!({ "app_name": app }).to_string();
                if crate::context_engine::operations::create_context_signal(
                    conn,
                    event.workspace_id.as_deref(),
                    Some(item_id),
                    "active_app",
                    event.session_id.as_deref(),
                    &app_payload,
                )
                .is_ok()
                {
                    count += 1;
                }
            }
        }

        // 3. Window Title Signal (If present and permitted by privacy)
        if settings.enable_window_title {
            if let Some(win) = &event.window_title {
                let win_payload = json!({ "window_title": win }).to_string();
                if crate::context_engine::operations::create_context_signal(
                    conn,
                    event.workspace_id.as_deref(),
                    Some(item_id),
                    "window_title",
                    event.session_id.as_deref(),
                    &win_payload,
                )
                .is_ok()
                {
                    count += 1;
                }
            }
        }

        // 4. URL Metadata Signal (If present and permitted by privacy)
        if settings.enable_browser_domain {
            if let Some(dom) = &event.domain {
                let url_payload = json!({
                    "domain": dom,
                    "original_url": event.original_url
                })
                .to_string();

                if crate::context_engine::operations::create_context_signal(
                    conn,
                    event.workspace_id.as_deref(),
                    Some(item_id),
                    "url_metadata",
                    event.session_id.as_deref(),
                    &url_payload,
                )
                .is_ok()
                {
                    count += 1;
                }
            }
        }

        // 5. File Metadata Signal (If file)
        if let Some(fname) = &event.filename {
            let file_payload = json!({
                "filename": fname,
                "mime_type": event.mime_type,
                "size": event.size_bytes
            })
            .to_string();

            if crate::context_engine::operations::create_context_signal(
                conn,
                event.workspace_id.as_deref(),
                Some(item_id),
                "file_metadata",
                event.session_id.as_deref(),
                &file_payload,
            )
            .is_ok()
            {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Step 5 implementation: Executes full capture pipeline within a live Tauri application environment.
    pub fn process_app(&self, app_handle: AppHandle, raw: RawCapturePayload) -> Result<CaptureResult, CaptureError> {
        // Step 1: Validate
        self.validate(&raw)?;

        // Step 2: Normalize
        let mut event = self.normalize(&raw)?;

        let state = app_handle.state::<AppState>();

        // Step 3 & 4: Persist Item and Signals with brief DB lock
        let (item, session_id, signals_emitted, enable_semantic) = {
            let conn = state
                .db_conn
                .lock()
                .map_err(|_| CaptureError::DatabaseError("Database lock poisoned".to_string()))?;

            let session_id_opt = {
                if let Ok(ce) = state.context_engine.lock() {
                    ce.get_or_update_active_session(&conn).ok()
                } else {
                    None
                }
            };
            event.session_id = session_id_opt.clone();

            let metadata_str = event.metadata.to_string();
            let item = operations::create_item(
                &conn,
                event.workspace_id.as_deref(),
                session_id_opt.as_deref(),
                &event.item_type,
                &event.content,
                event.title.as_deref(),
                &metadata_str,
            )
            .map_err(|e| CaptureError::DatabaseError(e.to_string()))?;

            let settings = crate::db::settings::get_settings(&conn).unwrap_or_default();
            let enable_semantic = settings.enable_semantic_processing;

            let semantic_status = if enable_semantic { "pending" } else { "failed" };
            let _ = operations::update_item_semantic_status(&conn, &item.id, semantic_status, None);

            let signals_count = self.generate_context_signals(&conn, &item.id, &event, &settings)?;

            (item, session_id_opt, signals_count, enable_semantic)
        };

        // Step 5: Feed into Context Engine & Trigger Async Semantic Embedding
        {
            let conn = state
                .db_conn
                .lock()
                .map_err(|_| CaptureError::DatabaseError("Database lock poisoned".to_string()))?;

            if let Ok(ce) = state.context_engine.lock() {
                ce.process_event(
                    &conn,
                    ContextEvent::Capture {
                        item_id: item.id.clone(),
                        workspace_id: event.workspace_id.clone(),
                        source: event.source_method.to_string(),
                        data_type: event.item_type.clone(),
                        metadata: event.metadata.clone(),
                    },
                );
            }

            let _ = crate::context_engine::relationships::generate_relationships(&conn, &item.id);
        }

        // Kick off async background semantic embedding
        if enable_semantic {
            let provider_opt = {
                if let Ok(ce) = state.context_engine.lock() {
                    ce.get_semantic_provider()
                } else {
                    None
                }
            };

            if let Some(provider) = provider_opt {
                let builder = crate::semantic::builder::SemanticContentBuilder::new();
                let text_to_embed = builder.build(&item);

                if !text_to_embed.trim().is_empty() {
                    let item_id = item.id.clone();
                    let app_handle_clone = app_handle.clone();

                    if let Ok(conn) = state.db_conn.lock() {
                        let _ = operations::update_item_semantic_status(&conn, &item_id, "processing", None);
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
                            if let Ok((embedding, model_name)) = result {
                                final_model = Some(model_name.clone());
                                if let Some(state) = app_handle_clone.try_state::<crate::AppState>() {
                                    let saved = if let Ok(conn) = state.db_conn.lock() {
                                        operations::save_item_embedding(
                                            &conn, &item_id, &embedding, &model_name, &model_name,
                                        )
                                        .is_ok()
                                    } else {
                                        false
                                    };

                                    if saved {
                                        if let Ok(ce) = state.context_engine.lock() {
                                            if let Ok(conn) = state.db_conn.lock() {
                                                let _ = crate::context_engine::relationships::generate_semantic_relationships(
                                                    &ce, &conn, &item_id,
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
                                    let _ = operations::update_item_semantic_status(
                                        &conn,
                                        &item_id,
                                        "ready",
                                        final_model.as_deref(),
                                    );
                                } else {
                                    let _ = operations::update_item_semantic_status(&conn, &item_id, "failed", None);
                                    let _ = operations::increment_semantic_retries(&conn, &item_id);
                                }
                            }
                        }
                    });
                } else if let Ok(conn) = state.db_conn.lock() {
                    let _ = operations::update_item_semantic_status(&conn, &item.id, "ready", None);
                }
            }
        }

        // Emit frontend notification event
        let _ = app_handle.emit("clipboard://new-item", &item);

        Ok(CaptureResult {
            item,
            signals_emitted,
            session_id,
            semantic_status: if enable_semantic { "pending" } else { "failed" }.to_string(),
            is_duplicate: false,
        })
    }
}

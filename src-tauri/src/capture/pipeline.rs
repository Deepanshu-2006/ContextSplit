use rusqlite::Connection;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use super::models::{
    CaptureError, CaptureEvent, CaptureResult, RawCapturePayload,
};
use super::normalizer::CaptureNormalizer;
use super::validator::CaptureValidator;
use crate::context_engine::engine::{ContextEngine, ContextEvent};
use crate::db::operations;
use crate::AppState;

/// Default deduplication time window (3 seconds).
pub const DEFAULT_DEDUP_WINDOW_SECS: u64 = 3;

/// The central 9-step cross-platform CapturePipeline.
///
/// Execution Stages:
/// 1. Receive CaptureEvent
/// 2. Validate it
/// 3. Normalize its content
/// 4. Deduplicate when appropriate
/// 5. Create the Item
/// 6. Create ContextSignals
/// 7. Associate the Item with the appropriate ContextSession
/// 8. Trigger contextual analysis asynchronously (non-blocking)
/// 9. Update the Inbox
#[derive(Debug, Clone)]
pub struct CapturePipeline {
    validator: CaptureValidator,
    normalizer: CaptureNormalizer,
    dedup_window_secs: u64,
}

impl Default for CapturePipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl CapturePipeline {
    /// Creates a new CapturePipeline with standard limits and deduplication.
    pub fn new() -> Self {
        Self {
            validator: CaptureValidator::new(),
            normalizer: CaptureNormalizer::new(),
            dedup_window_secs: DEFAULT_DEDUP_WINDOW_SECS,
        }
    }

    /// Sets custom validator.
    pub fn with_validator(mut self, validator: CaptureValidator) -> Self {
        self.validator = validator;
        self
    }

    /// Sets custom normalizer.
    pub fn with_normalizer(mut self, normalizer: CaptureNormalizer) -> Self {
        self.normalizer = normalizer;
        self
    }

    /// Configures the deduplication window in seconds.
    pub fn with_dedup_window_secs(mut self, secs: u64) -> Self {
        self.dedup_window_secs = secs;
        self
    }

    /// Disables deduplication.
    pub fn without_deduplication(mut self) -> Self {
        self.dedup_window_secs = 0;
        self
    }

    // =========================================================================
    // Core Pipeline Stages
    // =========================================================================

    /// Stage 1 & 2: Validate the CaptureEvent schema and integrity.
    pub fn validate(&self, event: &CaptureEvent) -> Result<(), CaptureError> {
        event.validate()
    }

    /// Stage 3: Normalize content, domain, MIME type, and metadata.
    pub fn normalize(&self, event: CaptureEvent) -> Result<CaptureEvent, CaptureError> {
        self.normalizer.normalize_event(event)
    }

    /// Stage 4: Checks whether an identical item was recently captured in the target workspace/inbox.
    pub fn check_duplicate(
        &self,
        conn: &Connection,
        event: &CaptureEvent,
    ) -> Result<Option<crate::db::models::Item>, CaptureError> {
        if self.dedup_window_secs == 0 {
            return Ok(None);
        }

        let trimmed_content = event.content.trim();
        let target_ws = event.workspace_id();

        let query = match &target_ws {
            Some(_ws_id) => {
                "SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at \
                 FROM items WHERE workspace_id = ?1 ORDER BY created_at DESC LIMIT 5"
            }
            None => {
                "SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at \
                 FROM items WHERE workspace_id IS NULL ORDER BY created_at DESC LIMIT 5"
            }
        };

        let mut stmt = conn
            .prepare(query)
            .map_err(|e| CaptureError::DatabaseError(e.to_string()))?;

        let map_fn = |row: &rusqlite::Row| -> rusqlite::Result<crate::db::models::Item> {
            Ok(crate::db::models::Item {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                session_id: row.get(2)?,
                item_type: row.get(3)?,
                content: row.get(4)?,
                title: row.get(5)?,
                metadata: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        };

        let items_iter = match &target_ws {
            Some(ws_id) => stmt.query_map(rusqlite::params![ws_id], map_fn),
            None => stmt.query_map([], map_fn),
        }
        .map_err(|e| CaptureError::DatabaseError(e.to_string()))?;

        let now = chrono::Utc::now();

        for item_res in items_iter {
            if let Ok(item) = item_res {
                if item.content.trim() == trimmed_content {
                    if let Ok(item_time) = chrono::DateTime::parse_from_rfc3339(&item.created_at) {
                        let diff = now.signed_duration_since(item_time.with_timezone(&chrono::Utc));
                        if diff.num_seconds().abs() <= (self.dedup_window_secs as i64) {
                            return Ok(Some(item));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Stage 6: Generates and records ContextSignals based on privacy settings.
    pub fn generate_context_signals(
        &self,
        conn: &Connection,
        item_id: &str,
        event: &CaptureEvent,
        session_id: Option<&str>,
        settings: &crate::db::settings::PrivacySettings,
    ) -> Result<usize, CaptureError> {
        let mut count = 0;
        let ws_id = event.workspace_id();

        // 1. Capture Event Signal (Always recorded)
        let capture_payload = json!({
            "event_id": event.event_id,
            "source": event.capture_type.to_string(),
            "platform": event.platform.to_string(),
            "data_type": event.item_type_str(),
            "content_type": event.content_type.to_string(),
            "size": event.size_bytes(),
            "captured_at": event.timestamp
        })
        .to_string();

        if crate::context_engine::operations::create_context_signal(
            conn,
            ws_id.as_deref(),
            Some(item_id),
            "capture_event",
            session_id,
            &capture_payload,
        )
        .is_ok()
        {
            count += 1;
        }

        // 2. Active App Signal (If present and permitted by privacy)
        if settings.enable_active_app {
            if let Some(app) = &event.source_application {
                let app_payload = json!({ "app_name": app }).to_string();
                if crate::context_engine::operations::create_context_signal(
                    conn,
                    ws_id.as_deref(),
                    Some(item_id),
                    "active_app",
                    session_id,
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
            if let Some(win) = &event.source_metadata.window_title {
                let win_payload = json!({ "window_title": win }).to_string();
                if crate::context_engine::operations::create_context_signal(
                    conn,
                    ws_id.as_deref(),
                    Some(item_id),
                    "window_title",
                    session_id,
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
            if let Some(dom) = event.domain() {
                let url_payload = json!({
                    "domain": dom,
                    "original_url": event.source_metadata.url_origin
                })
                .to_string();

                if crate::context_engine::operations::create_context_signal(
                    conn,
                    ws_id.as_deref(),
                    Some(item_id),
                    "url_metadata",
                    session_id,
                    &url_payload,
                )
                .is_ok()
                {
                    count += 1;
                }
            }
        }

        // 5. File Metadata Signal (If file)
        if let Some(fname) = event.filename() {
            let file_payload = json!({
                "filename": fname,
                "mime_type": event.mime_type(),
                "size": event.size_bytes()
            })
            .to_string();

            if crate::context_engine::operations::create_context_signal(
                conn,
                ws_id.as_deref(),
                Some(item_id),
                "file_metadata",
                session_id,
                &file_payload,
            )
            .is_ok()
            {
                count += 1;
            }
        }

        Ok(count)
    }

    // =========================================================================
    // Execution Modes
    // =========================================================================

    /// Executes the full 9-step pipeline in headless mode.
    pub fn process_event_headless(
        &self,
        conn: &Connection,
        context_engine: Option<&ContextEngine>,
        event: CaptureEvent,
    ) -> Result<CaptureResult, CaptureError> {
        // Step 1 & 2: Receive and Validate CaptureEvent
        self.validate(&event)?;

        // Step 3: Normalize content and metadata
        let mut normalized_event = self.normalize(event)?;

        // Step 4: Deduplicate when appropriate
        if let Some(existing_item) = self.check_duplicate(conn, &normalized_event)? {
            return Ok(CaptureResult {
                item: existing_item.clone(),
                signals_emitted: 0,
                session_id: existing_item.session_id,
                semantic_status: "ready".to_string(),
                is_duplicate: true,
            });
        }

        // Step 7 (prep): Associate with ContextSession
        let session_id = context_engine
            .and_then(|ce| ce.get_or_update_active_session(conn).ok())
            .or_else(|| normalized_event.session_id());

        if let Some(sess) = &session_id {
            if let Some(map) = normalized_event.metadata.as_object_mut() {
                map.insert("session_id".to_string(), json!(sess));
            }
        }

        // Step 5: Create the Item
        let metadata_str = normalized_event.metadata.to_string();
        let item_type = normalized_event.item_type_str().to_string();
        let workspace_id = normalized_event.workspace_id();
        let title = normalized_event.title();

        let item = operations::create_item(
            conn,
            workspace_id.as_deref(),
            session_id.as_deref(),
            &item_type,
            &normalized_event.content,
            title.as_deref(),
            &metadata_str,
        )
        .map_err(|e| CaptureError::DatabaseError(e.to_string()))?;

        // Read privacy & semantic settings
        let settings = crate::db::settings::get_settings(conn).unwrap_or_default();
        let enable_semantic = settings.enable_semantic_processing;

        let semantic_status = if enable_semantic { "pending" } else { "failed" };
        let _ = operations::update_item_semantic_status(conn, &item.id, semantic_status, None);

        // Step 6: Create ContextSignals
        let signals_emitted = self.generate_context_signals(
            conn,
            &item.id,
            &normalized_event,
            session_id.as_deref(),
            &settings,
        )?;

        // Step 8: Trigger contextual analysis
        if let Some(ce) = context_engine {
            ce.process_event(
                conn,
                ContextEvent::Capture {
                    item_id: item.id.clone(),
                    workspace_id: workspace_id.clone(),
                    source: normalized_event.capture_type.to_string(),
                    data_type: item_type,
                    metadata: normalized_event.metadata.clone(),
                },
            );

            // Generate deterministic relationships
            let _ = crate::context_engine::relationships::generate_relationships(conn, &item.id);

            // In headless/test mode, if semantic provider is configured, run embedding
            if enable_semantic {
                if let Some(provider) = ce.get_semantic_provider() {
                    let builder = crate::semantic::builder::SemanticContentBuilder::new();
                    let text_to_embed = builder.build(&item);
                    if !text_to_embed.trim().is_empty() {
                        if let Ok(emb) = provider.generate_embedding(&text_to_embed) {
                            let model = provider.status().model_name;
                            let _ = operations::save_item_embedding(conn, &item.id, &emb, &model, &model);
                            let _ = operations::update_item_semantic_status(conn, &item.id, "ready", Some(&model));
                            let _ = crate::context_engine::relationships::generate_semantic_relationships(
                                ce, conn, &item.id,
                            );
                        }
                    }
                }
            }
        }

        // Step 9: Update the Inbox (returns result with workspace_id == None or ws_id)
        Ok(CaptureResult {
            item,
            signals_emitted,
            session_id,
            semantic_status: semantic_status.to_string(),
            is_duplicate: false,
        })
    }

    /// Converts raw payload into CaptureEvent and runs headless execution.
    pub fn process_raw_headless(
        &self,
        conn: &Connection,
        context_engine: Option<&ContextEngine>,
        raw: RawCapturePayload,
    ) -> Result<CaptureResult, CaptureError> {
        self.validator.validate(&raw)?;
        let event = self.normalizer.normalize(&raw)?;
        self.process_event_headless(conn, context_engine, event)
    }

    /// Executes the full 9-step pipeline within a live Tauri application environment.
    ///
    /// GUARANTEE: The user-facing capture call returns in < 5ms.
    /// Embeddings, semantic analysis, and AI reasoning execute asynchronously in the background.
    pub fn process_event_app(
        &self,
        app_handle: AppHandle,
        event: CaptureEvent,
    ) -> Result<CaptureResult, CaptureError> {
        // Step 1 & 2: Validate
        self.validate(&event)?;

        // Step 3: Normalize content
        let mut normalized_event = self.normalize(event)?;

        let state = app_handle.state::<AppState>();

        // Step 4, 5, 6 & 7: Lock DB briefly for Item, Signals, and Session association
        let (item, session_id_opt, signals_emitted, enable_semantic, is_duplicate) = {
            let conn = state
                .db_conn
                .lock()
                .map_err(|_| CaptureError::DatabaseError("Database lock poisoned".to_string()))?;

            // Step 4: Check deduplication
            if let Some(existing_item) = self.check_duplicate(&conn, &normalized_event)? {
                return Ok(CaptureResult {
                    item: existing_item.clone(),
                    signals_emitted: 0,
                    session_id: existing_item.session_id,
                    semantic_status: "ready".to_string(),
                    is_duplicate: true,
                });
            }

            // Step 7: Associate with ContextSession
            let session_id_opt = {
                if let Ok(ce) = state.context_engine.lock() {
                    ce.get_or_update_active_session(&conn).ok()
                } else {
                    None
                }
            };

            if let Some(sess) = &session_id_opt {
                if let Some(map) = normalized_event.metadata.as_object_mut() {
                    map.insert("session_id".to_string(), json!(sess));
                }
            }

            // Step 5: Create Item
            let metadata_str = normalized_event.metadata.to_string();
            let item_type = normalized_event.item_type_str().to_string();
            let workspace_id = normalized_event.workspace_id();
            let title = normalized_event.title();

            let item = operations::create_item(
                &conn,
                workspace_id.as_deref(),
                session_id_opt.as_deref(),
                &item_type,
                &normalized_event.content,
                title.as_deref(),
                &metadata_str,
            )
            .map_err(|e| CaptureError::DatabaseError(e.to_string()))?;

            let settings = crate::db::settings::get_settings(&conn).unwrap_or_default();
            let enable_semantic = settings.enable_semantic_processing;

            let semantic_status = if enable_semantic { "pending" } else { "failed" };
            let _ = operations::update_item_semantic_status(&conn, &item.id, semantic_status, None);

            // Step 6: Create ContextSignals
            let signals_count = self.generate_context_signals(
                &conn,
                &item.id,
                &normalized_event,
                session_id_opt.as_deref(),
                &settings,
            )?;

            (item, session_id_opt, signals_count, enable_semantic, false)
        };

        // Fast in-memory event ingestion & basic relationships (< 1ms)
        {
            if let Ok(conn) = state.db_conn.lock() {
                if let Ok(ce) = state.context_engine.lock() {
                    ce.process_event(
                        &conn,
                        ContextEvent::Capture {
                            item_id: item.id.clone(),
                            workspace_id: normalized_event.workspace_id(),
                            source: normalized_event.capture_type.to_string(),
                            data_type: normalized_event.item_type_str().to_string(),
                            metadata: normalized_event.metadata.clone(),
                        },
                    );
                }
                let _ = crate::context_engine::relationships::generate_relationships(&conn, &item.id);
            }
        }

        // Step 8: Trigger Heavy Contextual Analysis Asynchronously (Non-blocking!)
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

                    // Async background spawn — does NOT block user response
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

        // Step 9: Update the Inbox and notify the frontend immediately
        let _ = app_handle.emit("clipboard://new-item", &item);
        let _ = app_handle.emit("inbox://updated", &item);
        let _ = app_handle.emit("capture://event", &normalized_event);

        Ok(CaptureResult {
            item,
            signals_emitted,
            session_id: session_id_opt,
            semantic_status: if enable_semantic { "pending" } else { "failed" }.to_string(),
            is_duplicate,
        })
    }

    /// Converts raw payload into CaptureEvent and runs live app execution.
    pub fn process_raw_app(
        &self,
        app_handle: AppHandle,
        raw: RawCapturePayload,
    ) -> Result<CaptureResult, CaptureError> {
        self.validator.validate(&raw)?;
        let event = self.normalizer.normalize(&raw)?;
        self.process_event_app(app_handle, event)
    }
}

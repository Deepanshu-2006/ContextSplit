use crate::capture::target::CaptureRequest;
use crate::db::models::Item;
use crate::context_engine::operations::create_context_signal;
use rusqlite::Connection;
use serde_json::json;

pub trait ContextProvider {
    fn name(&self) -> &'static str;
    fn collect(&self, req: &CaptureRequest, item: &Item) -> Option<serde_json::Value>;
}

pub struct CaptureEventProvider;

impl ContextProvider for CaptureEventProvider {
    fn name(&self) -> &'static str {
        "capture_event"
    }

    fn collect(&self, req: &CaptureRequest, _item: &Item) -> Option<serde_json::Value> {
        let mut source_metadata = serde_json::Map::new();
        
        if let Some(filename) = &req.filename {
            source_metadata.insert("filename".to_string(), json!(filename));
        }
        if let Some(mime_type) = &req.mime_type {
            source_metadata.insert("mime_type".to_string(), json!(mime_type));
        }
        if let Some(url) = &req.original_url {
            source_metadata.insert("original_url".to_string(), json!(url));
        }
        if let Some(size) = req.size {
            source_metadata.insert("size".to_string(), json!(size));
        }

        let capture_method = req.source.as_deref().unwrap_or("unknown");
        let content_type = &req.data_type;

        Some(json!({
            "capture_method": capture_method,
            "content_type": content_type,
            "source_metadata": source_metadata,
        }))
    }
}

pub fn collect_and_store_signals(conn: &Connection, req: &CaptureRequest, item: &Item) {
    let providers: Vec<Box<dyn ContextProvider>> = vec![
        Box::new(CaptureEventProvider),
    ];

    for provider in providers {
        if let Some(payload) = provider.collect(req, item) {
            let payload_str = payload.to_string();
            let signal_type = provider.name();
            let workspace_id = req.workspace_id.as_deref();
            let item_id = Some(item.id.as_str());
            let session_id = None;

            let _ = create_context_signal(
                conn,
                workspace_id,
                item_id,
                signal_type,
                session_id,
                &payload_str
            );
        }
    }
}

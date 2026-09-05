use serde_json::json;
use std::path::Path;

use super::models::{
    CaptureError, CaptureEvent, ContentType, RawCapturePayload, SourceMetadata,
};

/// Platform-independent normalizer for raw capture payloads.
#[derive(Debug, Clone, Default)]
pub struct CaptureNormalizer;

impl CaptureNormalizer {
    pub fn new() -> Self {
        Self
    }

    /// Normalizes a validated raw capture payload into a structured `CaptureEvent`.
    pub fn normalize(&self, payload: &RawCapturePayload) -> Result<CaptureEvent, CaptureError> {
        if payload.source_method == super::models::CaptureType::DragDrop {
            return super::drag_normalizer::DragPayloadNormalizer::new().normalize_raw_payload(payload);
        }

        let now = chrono::Utc::now().to_rfc3339();

        // 1. Determine or infer Item / Content Type
        let raw_content = payload.content.trim();
        let content_type = self.infer_content_type(payload, raw_content);

        // 2. Normalize content, domain, filename, mime_type, and title based on type
        let mut domain: Option<String> = None;
        let mut filename: Option<String> = None;
        let mut mime_type: Option<String> = payload.mime_type.clone();
        let mut original_url: Option<String> = payload.original_url.clone();
        let mut normalized_content = raw_content.to_string();
        let mut title = payload
            .title
            .as_deref()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        match &content_type {
            ContentType::Url => {
                let cleaned_url = self.normalize_url(raw_content);
                domain = self.extract_domain(&cleaned_url);
                original_url = Some(cleaned_url.clone());
                normalized_content = cleaned_url.clone();
                if mime_type.is_none() {
                    mime_type = Some("text/uri-list".to_string());
                }
                if title.is_none() {
                    title = self.infer_url_title(
                        &cleaned_url,
                        domain.as_deref(),
                        payload.window_title.as_deref(),
                    );
                }
            }
            ContentType::File => {
                let path_to_use = if !payload.file_paths.is_empty() {
                    &payload.file_paths[0]
                } else {
                    raw_content
                };
                let path_obj = Path::new(path_to_use);
                let fname = path_obj
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("unnamed_file")
                    .to_string();
                filename = Some(fname.clone());
                if mime_type.is_none() {
                    mime_type = Some(self.infer_mime_type_from_path(path_to_use));
                }
                if title.is_none() {
                    title = Some(fname);
                }
                normalized_content = path_to_use.to_string();
            }
            ContentType::Image => {
                if mime_type.is_none() {
                    mime_type = Some("image/png".to_string());
                }
                if title.is_none() {
                    title = Some(if let Some(app) = &payload.app_name {
                        format!("Image from {}", app)
                    } else {
                        "Captured Image".to_string()
                    });
                }
            }
            ContentType::RichText => {
                normalized_content = raw_content.replace("\r\n", "\n");
                if mime_type.is_none() {
                    mime_type = Some("text/rtf".to_string());
                }
                if title.is_none() {
                    title = self.infer_text_title(&normalized_content, payload.window_title.as_deref());
                }
            }
            _ => {
                // Plain text or other
                normalized_content = raw_content.replace("\r\n", "\n");
                if mime_type.is_none() {
                    mime_type = Some("text/plain".to_string());
                }
                if title.is_none() {
                    title = self.infer_text_title(&normalized_content, payload.window_title.as_deref());
                }
            }
        }

        // 3. Calculate size
        let size_bytes = payload.size_bytes.unwrap_or(normalized_content.len());

        // 4. Construct metadata JSON map
        let mut meta_map = serde_json::Map::new();
        meta_map.insert("source".to_string(), json!(payload.source_method.to_string()));
        meta_map.insert("platform".to_string(), json!(payload.platform.to_string()));
        meta_map.insert("size".to_string(), json!(size_bytes));
        meta_map.insert("content_type".to_string(), json!(content_type.to_string()));

        if let Some(t) = &title {
            meta_map.insert("title".to_string(), json!(t));
        }
        if let Some(ws) = &payload.workspace_id {
            meta_map.insert("workspace_id".to_string(), json!(ws));
        }
        if let Some(app) = &payload.app_name {
            meta_map.insert("app_name".to_string(), json!(app));
        }
        if let Some(win) = &payload.window_title {
            meta_map.insert("window_title".to_string(), json!(win));
        }
        if let Some(proc) = &payload.process_info {
            meta_map.insert("process_info".to_string(), json!(proc));
        }
        if let Some(dom) = &domain {
            meta_map.insert("domain".to_string(), json!(dom));
        }
        if let Some(fname) = &filename {
            meta_map.insert("filename".to_string(), json!(fname));
        }
        if let Some(mime) = &mime_type {
            meta_map.insert("mime_type".to_string(), json!(mime));
        }
        if let Some(orig) = &original_url {
            meta_map.insert("original_url".to_string(), json!(orig));
        }
        if let Some(raw_m) = &payload.raw_metadata {
            if let Some(obj) = raw_m.as_object() {
                for (k, v) in obj {
                    if !meta_map.contains_key(k) {
                        meta_map.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        let source_metadata = SourceMetadata {
            window_title: payload.window_title.clone(),
            process_name: payload.process_info.clone(),
            bundle_identifier: None,
            url_origin: original_url,
            extra: json!({}),
        };

        Ok(CaptureEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            capture_type: payload.source_method.clone(),
            timestamp: now,
            content_type,
            content: normalized_content,
            source_application: payload.app_name.clone(),
            source_metadata,
            platform: payload.platform,
            metadata: serde_json::Value::Object(meta_map),
        })
    }

    /// Infers ContentType enum from payload and content.
    fn infer_content_type(&self, payload: &RawCapturePayload, content: &str) -> ContentType {
        if let Some(explicit) = &payload.data_type {
            let lower = explicit.to_lowercase();
            match lower.as_str() {
                "url" | "uri" => return ContentType::Url,
                "image" | "png" | "jpeg" | "jpg" | "webp" => return ContentType::Image,
                "file" => return ContentType::File,
                "rich_text" | "rtf" | "html" => return ContentType::RichText,
                "plain_text" | "text" | "code" => return ContentType::PlainText,
                "unknown" => return ContentType::Unknown,
                other => return ContentType::Custom(other.to_string()),
            }
        }

        if !payload.file_paths.is_empty() {
            return ContentType::File;
        }

        if content.starts_with("data:image/") {
            return ContentType::Image;
        }

        if content.starts_with("http://") || content.starts_with("https://") {
            return ContentType::Url;
        }

        if content.starts_with("file://") {
            return ContentType::File;
        }

        // Check if content is a single existing filesystem path
        if content.starts_with('/')
            || (content.len() > 3
                && content.chars().nth(1) == Some(':')
                && content.chars().nth(2) == Some('\\'))
        {
            if Path::new(content).exists() {
                return ContentType::File;
            }
        }

        ContentType::PlainText
    }

    /// Cleans and normalizes URL string.
    fn normalize_url(&self, raw_url: &str) -> String {
        let trimmed = raw_url.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("file://") {
            trimmed.to_string()
        } else {
            format!("https://{}", trimmed)
        }
    }

    /// Extracts domain from a normalized URL.
    pub fn extract_domain(&self, url_str: &str) -> Option<String> {
        let stripped = if let Some(s) = url_str.strip_prefix("https://") {
            s
        } else if let Some(s) = url_str.strip_prefix("http://") {
            s
        } else if let Some(_s) = url_str.strip_prefix("file://") {
            return None;
        } else {
            url_str
        };

        let host_part = stripped.split('/').next().unwrap_or(stripped);
        let host_clean = host_part.split(':').next().unwrap_or(host_part);
        let domain = host_clean.trim_start_matches("www.").to_lowercase();

        if domain.is_empty() || !domain.contains('.') {
            None
        } else {
            Some(domain)
        }
    }

    /// Infers a human-readable title for a URL.
    fn infer_url_title(&self, url: &str, domain: Option<&str>, window_title: Option<&str>) -> Option<String> {
        if let Some(win) = window_title {
            let win_clean = win.trim();
            if !win_clean.is_empty() {
                return Some(win_clean.to_string());
            }
        }

        if let Some(dom) = domain {
            // Capitalize domain name (e.g. booking.com -> Booking.com)
            let mut chars = dom.chars();
            if let Some(first) = chars.next() {
                let capitalized = format!("{}{}", first.to_uppercase(), chars.as_str());
                return Some(format!("Link from {}", capitalized));
            }
        }

        Some(url.chars().take(50).collect())
    }

    /// Infers title from the first line of text content.
    fn infer_text_title(&self, text: &str, window_title: Option<&str>) -> Option<String> {
        if let Some(win) = window_title {
            let win_clean = win.trim();
            if !win_clean.is_empty() && win_clean.chars().count() <= 60 {
                return Some(win_clean.to_string());
            }
        }

        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if trimmed.chars().count() > 50 {
                    let truncated: String = trimmed.chars().take(47).collect();
                    return Some(format!("{}...", truncated));
                }
                return Some(trimmed.to_string());
            }
        }

        None
    }

    /// Infers MIME type from file extension.
    fn infer_mime_type_from_path(&self, path: &str) -> String {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "json" => "application/json",
            "md" | "markdown" => "text/markdown",
            "txt" => "text/plain",
            "rs" => "text/x-rust",
            "ts" | "tsx" => "text/typescript",
            "js" | "jsx" => "text/javascript",
            "html" => "text/html",
            "css" => "text/css",
            "csv" => "text/csv",
            "zip" => "application/zip",
            _ => "application/octet-stream",
        }
        .to_string()
    }

    /// Normalizes a pre-constructed `CaptureEvent` ensuring consistent whitespace,
    /// CRLF normalization, domain extraction, filename detection, MIME resolution, and metadata mapping.
    pub fn normalize_event(&self, mut event: CaptureEvent) -> Result<CaptureEvent, CaptureError> {
        let raw_content = event.content.trim();
        let mut normalized_content = raw_content.replace("\r\n", "\n");

        let mut meta_map = match event.metadata.as_object() {
            Some(obj) => obj.clone(),
            None => serde_json::Map::new(),
        };

        // Ensure baseline metadata fields
        meta_map.insert("source".to_string(), json!(event.capture_type.to_string()));
        meta_map.insert("platform".to_string(), json!(event.platform.to_string()));
        meta_map.insert("content_type".to_string(), json!(event.content_type.to_string()));

        if let Some(app) = &event.source_application {
            meta_map.entry("app_name".to_string()).or_insert_with(|| json!(app));
        }
        if let Some(win) = &event.source_metadata.window_title {
            meta_map.entry("window_title".to_string()).or_insert_with(|| json!(win));
        }
        if let Some(proc) = &event.source_metadata.process_name {
            meta_map.entry("process_info".to_string()).or_insert_with(|| json!(proc));
        }

        match &event.content_type {
            ContentType::Url => {
                let cleaned_url = self.normalize_url(&normalized_content);
                let domain = self.extract_domain(&cleaned_url);
                if let Some(dom) = &domain {
                    meta_map.entry("domain".to_string()).or_insert_with(|| json!(dom));
                }
                meta_map.entry("mime_type".to_string()).or_insert_with(|| json!("text/uri-list"));
                meta_map.entry("original_url".to_string()).or_insert_with(|| json!(&cleaned_url));
                if !meta_map.contains_key("title") {
                    if let Some(t) = self.infer_url_title(&cleaned_url, domain.as_deref(), event.source_metadata.window_title.as_deref()) {
                        meta_map.insert("title".to_string(), json!(t));
                    }
                }
                normalized_content = cleaned_url;
            }
            ContentType::File => {
                let path_obj = Path::new(&normalized_content);
                let fname = path_obj
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("unnamed_file")
                    .to_string();
                meta_map.entry("filename".to_string()).or_insert_with(|| json!(&fname));
                meta_map.entry("mime_type".to_string()).or_insert_with(|| json!(self.infer_mime_type_from_path(&normalized_content)));
                if !meta_map.contains_key("title") {
                    meta_map.insert("title".to_string(), json!(fname));
                }
            }
            ContentType::Image => {
                meta_map.entry("mime_type".to_string()).or_insert_with(|| json!("image/png"));
                if !meta_map.contains_key("title") {
                    let title = if let Some(app) = &event.source_application {
                        format!("Image from {}", app)
                    } else {
                        "Captured Image".to_string()
                    };
                    meta_map.insert("title".to_string(), json!(title));
                }
            }
            ContentType::RichText => {
                meta_map.entry("mime_type".to_string()).or_insert_with(|| json!("text/rtf"));
                if !meta_map.contains_key("title") {
                    if let Some(t) = self.infer_text_title(&normalized_content, event.source_metadata.window_title.as_deref()) {
                        meta_map.insert("title".to_string(), json!(t));
                    }
                }
            }
            _ => {
                meta_map.entry("mime_type".to_string()).or_insert_with(|| json!("text/plain"));
                if !meta_map.contains_key("title") {
                    if let Some(t) = self.infer_text_title(&normalized_content, event.source_metadata.window_title.as_deref()) {
                        meta_map.insert("title".to_string(), json!(t));
                    }
                }
            }
        }

        meta_map.insert("size".to_string(), json!(normalized_content.len()));

        event.content = normalized_content;
        event.metadata = serde_json::Value::Object(meta_map);

        Ok(event)
    }
}

use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_json::json;
use base64::{engine::general_purpose, Engine as _};

use super::models::{
    CaptureError, CaptureEvent, CaptureType, ContentType, PlatformDragDropInput,
    PlatformType, RawCapturePayload, SourceMetadata,
};
use super::target::CaptureRequest;

/// Raw representation of platform drag-and-drop payloads before normalization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawDragPayload {
    /// Main string content (text, url, data URI, file path, html snippet, etc.)
    pub content: Option<String>,
    /// List of dropped file paths (e.g. from Explorer, Finder, or file manager)
    pub file_paths: Vec<String>,
    /// Raw HTML snippet if dropped from a web browser or rich text editor
    pub html: Option<String>,
    /// Raw RTF content if dropped from a rich document editor
    pub rtf: Option<String>,
    /// Plain text representation or fallback
    pub text: Option<String>,
    /// Dropped URL (e.g. from browser address bar or link drag)
    pub url: Option<String>,
    /// Raw image bytes if dropped directly as an image buffer
    pub image_bytes: Option<Vec<u8>>,
    /// Image dimensions (width, height) if known
    pub image_dimensions: Option<(u32, u32)>,
    /// Explicit or suggested data type (e.g. "url", "text", "file", "image", "rich_text")
    pub data_type: Option<String>,
    /// MIME type if provided by drag source (e.g. "text/uri-list", "text/html", "image/png")
    pub mime_type: Option<String>,
    /// Custom or user-specified title
    pub title: Option<String>,
    /// Originating application name
    pub app_name: Option<String>,
    /// Originating window title
    pub window_title: Option<String>,
    /// Originating process name or identifier
    pub process_info: Option<String>,
    /// Destination workspace ID (None for Inbox)
    pub workspace_id: Option<String>,
    /// Originating platform (macOS, Windows, Linux, Generic)
    pub platform: Option<PlatformType>,
    /// Original URL if captured from browser
    pub original_url: Option<String>,
    /// Explicit size in bytes if known
    pub size_bytes: Option<usize>,
    /// Arbitrary platform metadata
    pub raw_metadata: Option<serde_json::Value>,
}

impl RawDragPayload {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_content(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ..Default::default()
        }
    }

    pub fn from_files(paths: Vec<String>) -> Self {
        Self {
            file_paths: paths,
            ..Default::default()
        }
    }

    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            url: Some(url.into()),
            ..Default::default()
        }
    }

    pub fn from_html(html: impl Into<String>) -> Self {
        Self {
            html: Some(html.into()),
            ..Default::default()
        }
    }

    pub fn from_rtf(rtf: impl Into<String>) -> Self {
        Self {
            rtf: Some(rtf.into()),
            ..Default::default()
        }
    }

    pub fn from_image_bytes(bytes: Vec<u8>) -> Self {
        Self {
            image_bytes: Some(bytes),
            ..Default::default()
        }
    }

    pub fn with_app_info(mut self, app_name: Option<String>, window_title: Option<String>) -> Self {
        self.app_name = app_name;
        self.window_title = window_title;
        self
    }

    pub fn with_workspace(mut self, workspace_id: Option<String>) -> Self {
        self.workspace_id = workspace_id;
        self
    }

    pub fn with_platform(mut self, platform: PlatformType) -> Self {
        self.platform = Some(platform);
        self
    }
}

/// Platform-independent Drag Payload Normalizer.
///
/// Detects and normalizes the 5 core payload types:
/// 1. URL
/// 2. Plain Text
/// 3. File
/// 4. Image
/// 5. Rich Text
///
/// Converts platform-specific drag payloads into canonical, validated `CaptureEvent`s.
#[derive(Debug, Clone, Default)]
pub struct DragPayloadNormalizer;

impl DragPayloadNormalizer {
    pub fn new() -> Self {
        Self
    }

    /// Normalizes a `RawDragPayload` into a canonical `CaptureEvent`.
    pub fn normalize_drag_payload(&self, payload: &RawDragPayload) -> Result<CaptureEvent, CaptureError> {
        let platform = payload.platform.unwrap_or_else(PlatformType::current);

        // 1. Identify Content Type and primary content
        let (content_type, mut normalized_content, mut mime_type, mut filename, mut original_url) =
            self.detect_and_prepare(payload)?;

        // 2. Clean content based on type
        match &content_type {
            ContentType::Url => {
                let cleaned_url = self.clean_url(&normalized_content);
                normalized_content = cleaned_url.clone();
                original_url = Some(cleaned_url);
                if mime_type.is_none() {
                    mime_type = Some("text/uri-list".to_string());
                }
            }
            ContentType::File => {
                let cleaned_path = self.clean_file_path(&normalized_content);
                let fname = cleaned_path
                    .split(|c| c == '/' || c == '\\')
                    .filter(|s| !s.is_empty())
                    .last()
                    .unwrap_or("unnamed_file")
                    .to_string();
                filename = Some(fname);
                if mime_type.is_none() {
                    mime_type = Some(self.infer_mime_type_from_path(&cleaned_path));
                }
                normalized_content = cleaned_path;
            }
            ContentType::Image => {
                if !normalized_content.starts_with("data:image/") {
                    let cleaned_path = self.clean_file_path(&normalized_content);
                    let fname = cleaned_path
                        .split(|c| c == '/' || c == '\\')
                        .filter(|s| !s.is_empty())
                        .last()
                        .unwrap_or("image.png")
                        .to_string();
                    filename = Some(fname);
                    if mime_type.is_none() {
                        mime_type = Some(self.infer_mime_type_from_path(&cleaned_path));
                    }
                    normalized_content = cleaned_path;
                } else if mime_type.is_none() {
                    mime_type = Some("image/png".to_string());
                }
            }
            ContentType::RichText => {
                normalized_content = normalized_content.replace("\r\n", "\n");
                if mime_type.is_none() {
                    mime_type = Some(if normalized_content.starts_with("{\\rtf") {
                        "text/rtf".to_string()
                    } else {
                        "text/html".to_string()
                    });
                }
            }
            ContentType::PlainText | ContentType::Unknown | ContentType::Custom(_) => {
                normalized_content = normalized_content.replace("\r\n", "\n");
                if mime_type.is_none() {
                    mime_type = Some("text/plain".to_string());
                }
            }
        }

        // 3. Domain extraction for URLs
        let mut domain: Option<String> = None;
        if let Some(ref orig) = original_url {
            domain = self.extract_domain(orig);
        } else if content_type == ContentType::Url {
            domain = self.extract_domain(&normalized_content);
        }

        // 4. Title inference
        let title = if let Some(ref t) = payload.title {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                self.infer_title(&content_type, &normalized_content, filename.as_deref(), domain.as_deref(), payload.window_title.as_deref(), payload.app_name.as_deref())
            } else {
                Some(trimmed.to_string())
            }
        } else {
            self.infer_title(&content_type, &normalized_content, filename.as_deref(), domain.as_deref(), payload.window_title.as_deref(), payload.app_name.as_deref())
        };

        // 5. Size calculation
        let size_bytes = payload.size_bytes.unwrap_or(normalized_content.len());

        // 6. Build Metadata Map
        let mut meta_map = serde_json::Map::new();
        meta_map.insert("source".to_string(), json!(CaptureType::DragDrop.to_string()));
        meta_map.insert("platform".to_string(), json!(platform.to_string()));
        meta_map.insert("size".to_string(), json!(size_bytes));
        meta_map.insert("content_type".to_string(), json!(content_type.to_string()));

        if let Some(ref t) = title {
            meta_map.insert("title".to_string(), json!(t));
        }
        if let Some(ref ws) = payload.workspace_id {
            meta_map.insert("workspace_id".to_string(), json!(ws));
        }
        if let Some(ref app) = payload.app_name {
            meta_map.insert("app_name".to_string(), json!(app));
        }
        if let Some(ref win) = payload.window_title {
            meta_map.insert("window_title".to_string(), json!(win));
        }
        if let Some(ref proc) = payload.process_info {
            meta_map.insert("process_info".to_string(), json!(proc));
        }
        if let Some(ref dom) = domain {
            meta_map.insert("domain".to_string(), json!(dom));
        }
        if let Some(ref fname) = filename {
            meta_map.insert("filename".to_string(), json!(fname));
        }
        if let Some(ref mime) = mime_type {
            meta_map.insert("mime_type".to_string(), json!(mime));
        }
        if let Some(ref orig) = original_url {
            meta_map.insert("original_url".to_string(), json!(orig));
        }
        if !payload.file_paths.is_empty() {
            meta_map.insert("file_paths".to_string(), json!(payload.file_paths));
        }
        if let Some(ref raw_m) = payload.raw_metadata {
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

        let event = CaptureEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            capture_type: CaptureType::DragDrop,
            timestamp: chrono::Utc::now().to_rfc3339(),
            content_type,
            content: normalized_content,
            source_application: payload.app_name.clone(),
            source_metadata,
            platform,
            metadata: serde_json::Value::Object(meta_map),
        };

        // Validate event before returning
        event.validate()?;

        Ok(event)
    }

    /// Converts `PlatformDragDropInput` into a normalized `CaptureEvent`.
    pub fn normalize_platform_input(&self, input: PlatformDragDropInput, platform: PlatformType) -> Result<CaptureEvent, CaptureError> {
        let payload = RawDragPayload {
            content: None,
            file_paths: input.paths,
            html: None,
            rtf: None,
            text: input.text,
            url: input.url,
            image_bytes: None,
            image_dimensions: None,
            data_type: None,
            mime_type: None,
            title: None,
            app_name: input.active_app,
            window_title: input.window_title,
            process_info: None,
            workspace_id: input.workspace_id,
            platform: Some(platform),
            original_url: None,
            size_bytes: None,
            raw_metadata: None,
        };

        self.normalize_drag_payload(&payload)
    }

    /// Converts `CaptureRequest` from UI drag/drop into a normalized `CaptureEvent`.
    pub fn normalize_capture_request(&self, req: &CaptureRequest, platform: PlatformType) -> Result<CaptureEvent, CaptureError> {
        let payload = RawDragPayload {
            content: Some(req.content.clone()),
            file_paths: req.filename.as_ref().map(|f| vec![f.clone()]).unwrap_or_default(),
            html: None,
            rtf: None,
            text: None,
            url: req.original_url.clone(),
            image_bytes: None,
            image_dimensions: None,
            data_type: Some(req.data_type.clone()),
            mime_type: req.mime_type.clone(),
            title: req.title.clone(),
            app_name: None,
            window_title: None,
            process_info: None,
            workspace_id: req.workspace_id.clone(),
            platform: Some(platform),
            original_url: req.original_url.clone(),
            size_bytes: req.size,
            raw_metadata: None,
        };

        self.normalize_drag_payload(&payload)
    }

    /// Converts `RawCapturePayload` into a normalized `CaptureEvent`.
    pub fn normalize_raw_payload(&self, raw: &RawCapturePayload) -> Result<CaptureEvent, CaptureError> {
        let payload = RawDragPayload {
            content: Some(raw.content.clone()),
            file_paths: raw.file_paths.clone(),
            html: None,
            rtf: None,
            text: None,
            url: raw.original_url.clone(),
            image_bytes: None,
            image_dimensions: None,
            data_type: raw.data_type.clone(),
            mime_type: raw.mime_type.clone(),
            title: raw.title.clone(),
            app_name: raw.app_name.clone(),
            window_title: raw.window_title.clone(),
            process_info: raw.process_info.clone(),
            workspace_id: raw.workspace_id.clone(),
            platform: Some(raw.platform),
            original_url: raw.original_url.clone(),
            size_bytes: raw.size_bytes,
            raw_metadata: raw.raw_metadata.clone(),
        };

        self.normalize_drag_payload(&payload)
    }

    // =========================================================================
    // Detection & Extraction Helpers
    // =========================================================================

    fn detect_and_prepare(
        &self,
        payload: &RawDragPayload,
    ) -> Result<(ContentType, String, Option<String>, Option<String>, Option<String>), CaptureError> {
        // 1. Check raw image bytes
        if let Some(ref bytes) = payload.image_bytes {
            if !bytes.is_empty() {
                let b64 = general_purpose::STANDARD.encode(bytes);
                let content = format!("data:image/png;base64,{}", b64);
                return Ok((
                    ContentType::Image,
                    content,
                    payload.mime_type.clone().or_else(|| Some("image/png".to_string())),
                    None,
                    None,
                ));
            }
        }

        // 1b. Check if content is already a data URI image
        if let Some(ref content_str) = payload.content {
            if content_str.starts_with("data:image/") {
                let mime = payload.mime_type.clone().or_else(|| {
                    if let Some(idx) = content_str.find(';') {
                        Some(content_str[5..idx].to_string())
                    } else {
                        Some("image/png".to_string())
                    }
                });
                let filename = payload.file_paths.first().map(|p| {
                    Path::new(p).file_name().and_then(|f| f.to_str()).unwrap_or("image.png").to_string()
                }).or_else(|| payload.title.clone());
                return Ok((
                    ContentType::Image,
                    content_str.clone(),
                    mime,
                    filename,
                    None,
                ));
            }
        }

        // 2. Check dropped files
        if !payload.file_paths.is_empty() {
            let primary_path = payload.file_paths[0].trim();
            if !primary_path.is_empty() {
                let cleaned = self.clean_file_path(primary_path);
                let is_img = self.is_image_extension(&cleaned);
                let content_type = if is_img && payload.data_type.as_deref() == Some("image") {
                    ContentType::Image
                } else if is_img {
                    ContentType::Image
                } else {
                    ContentType::File
                };
                let mime = payload.mime_type.clone().or_else(|| Some(self.infer_mime_type_from_path(&cleaned)));
                return Ok((content_type, cleaned, mime, None, None));
            }
        }

        // 3. Check explicit URL field
        if let Some(ref url) = payload.url {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                if trimmed.starts_with("file://") {
                    let cleaned = self.clean_file_path(trimmed);
                    let is_img = self.is_image_extension(&cleaned);
                    let ct = if is_img { ContentType::Image } else { ContentType::File };
                    return Ok((ct, cleaned.clone(), Some(self.infer_mime_type_from_path(&cleaned)), None, None));
                } else {
                    return Ok((
                        ContentType::Url,
                        trimmed.to_string(),
                        payload.mime_type.clone().or_else(|| Some("text/uri-list".to_string())),
                        None,
                        Some(trimmed.to_string()),
                    ));
                }
            }
        }

        // 4. Check RTF field
        if let Some(ref rtf) = payload.rtf {
            let trimmed = rtf.trim();
            if !trimmed.is_empty() {
                return Ok((
                    ContentType::RichText,
                    trimmed.to_string(),
                    payload.mime_type.clone().or_else(|| Some("text/rtf".to_string())),
                    None,
                    None,
                ));
            }
        }

        // 5. Check HTML field
        if let Some(ref html) = payload.html {
            let trimmed = html.trim();
            if !trimmed.is_empty() {
                return Ok((
                    ContentType::RichText,
                    trimmed.to_string(),
                    payload.mime_type.clone().or_else(|| Some("text/html".to_string())),
                    None,
                    None,
                ));
            }
        }

        // 6. Inspect `content` or fallback `text`
        let raw_str = payload
            .content
            .as_deref()
            .or_else(|| payload.text.as_deref())
            .unwrap_or("")
            .trim();

        if raw_str.is_empty() {
            return Err(CaptureError::EmptyContent);
        }

        // Explicit data_type hint override
        if let Some(ref dt) = payload.data_type {
            match dt.to_lowercase().as_str() {
                "url" | "uri" => {
                    return Ok((
                        ContentType::Url,
                        raw_str.to_string(),
                        payload.mime_type.clone().or_else(|| Some("text/uri-list".to_string())),
                        None,
                        Some(raw_str.to_string()),
                    ));
                }
                "image" | "png" | "jpeg" | "jpg" | "webp" => {
                    return Ok((
                        ContentType::Image,
                        raw_str.to_string(),
                        payload.mime_type.clone().or_else(|| Some("image/png".to_string())),
                        None,
                        None,
                    ));
                }
                "file" => {
                    let cleaned = self.clean_file_path(raw_str);
                    return Ok((
                        ContentType::File,
                        cleaned.clone(),
                        payload.mime_type.clone().or_else(|| Some(self.infer_mime_type_from_path(&cleaned))),
                        None,
                        None,
                    ));
                }
                "rich_text" | "rtf" | "html" => {
                    return Ok((
                        ContentType::RichText,
                        raw_str.to_string(),
                        payload.mime_type.clone().or_else(|| Some("text/html".to_string())),
                        None,
                        None,
                    ));
                }
                "plain_text" | "text" => {
                    return Ok((
                        ContentType::PlainText,
                        raw_str.to_string(),
                        payload.mime_type.clone().or_else(|| Some("text/plain".to_string())),
                        None,
                        None,
                    ));
                }
                _ => {}
            }
        }

        // Heuristic detection on raw_str:
        // A. Image Data URI
        if raw_str.starts_with("data:image/") {
            let mime = raw_str
                .strip_prefix("data:")
                .and_then(|s| s.split(';').next())
                .unwrap_or("image/png")
                .to_string();
            return Ok((ContentType::Image, raw_str.to_string(), Some(mime), None, None));
        }

        // B. File URI (file://...)
        if raw_str.starts_with("file://") {
            let cleaned = self.clean_file_path(raw_str);
            let is_img = self.is_image_extension(&cleaned);
            let ct = if is_img { ContentType::Image } else { ContentType::File };
            return Ok((ct, cleaned.clone(), Some(self.infer_mime_type_from_path(&cleaned)), None, None));
        }

        // C. Web URL (http:// or https:// or ftp://)
        if raw_str.starts_with("http://") || raw_str.starts_with("https://") || raw_str.starts_with("ftp://") {
            return Ok((
                ContentType::Url,
                raw_str.to_string(),
                payload.mime_type.clone().or_else(|| Some("text/uri-list".to_string())),
                None,
                Some(raw_str.to_string()),
            ));
        }

        // D. Windows / Unix local absolute path
        if self.is_filesystem_path(raw_str) {
            let cleaned = self.clean_file_path(raw_str);
            let is_img = self.is_image_extension(&cleaned);
            let ct = if is_img { ContentType::Image } else { ContentType::File };
            return Ok((ct, cleaned.clone(), Some(self.infer_mime_type_from_path(&cleaned)), None, None));
        }

        // E. RTF
        if raw_str.starts_with("{\\rtf") {
            return Ok((
                ContentType::RichText,
                raw_str.to_string(),
                payload.mime_type.clone().or_else(|| Some("text/rtf".to_string())),
                None,
                None,
            ));
        }

        // F. HTML content
        if self.is_html_content(raw_str) {
            return Ok((
                ContentType::RichText,
                raw_str.to_string(),
                payload.mime_type.clone().or_else(|| Some("text/html".to_string())),
                None,
                None,
            ));
        }

        // G. URL starting with www.
        if raw_str.starts_with("www.") && raw_str.contains('.') && !raw_str.contains(' ') {
            let full_url = format!("https://{}", raw_str);
            return Ok((
                ContentType::Url,
                full_url.clone(),
                payload.mime_type.clone().or_else(|| Some("text/uri-list".to_string())),
                None,
                Some(full_url),
            ));
        }

        // H. Default: Plain Text
        Ok((
            ContentType::PlainText,
            raw_str.to_string(),
            payload.mime_type.clone().or_else(|| Some("text/plain".to_string())),
            None,
            None,
        ))
    }

    /// Cleans and normalizes URL string.
    pub fn clean_url(&self, url: &str) -> String {
        let trimmed = url.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("ftp://") || trimmed.starts_with("file://") {
            trimmed.to_string()
        } else {
            format!("https://{}", trimmed)
        }
    }

    /// Cleans percent-encoded `file://` URLs or Windows quoted paths into canonical local filesystem paths.
    pub fn clean_file_path(&self, raw_path: &str) -> String {
        let trimmed = raw_path.trim().trim_matches('"');
        
        let path_without_scheme = if let Some(s) = trimmed.strip_prefix("file://localhost") {
            s
        } else if let Some(s) = trimmed.strip_prefix("file://") {
            s
        } else {
            trimmed
        };

        let decoded = self.percent_decode(path_without_scheme);

        // On Windows format like "/C:/Users/..." -> "C:\Users\..."
        if decoded.len() >= 3 && decoded.starts_with('/') && decoded.chars().nth(2) == Some(':') {
            let without_slash = &decoded[1..];
            without_slash.replace('/', "\\")
        } else if decoded.len() >= 2 && decoded.chars().nth(1) == Some(':') {
            decoded.replace('/', "\\")
        } else {
            decoded
        }
    }

    fn percent_decode(&self, input: &str) -> String {
        let mut bytes = Vec::new();
        let mut chars = input.bytes();
        while let Some(b) = chars.next() {
            if b == b'%' {
                let h1 = chars.next().unwrap_or(b'0');
                let h2 = chars.next().unwrap_or(b'0');
                if let Ok(val) = u8::from_str_radix(&format!("{}{}", h1 as char, h2 as char), 16) {
                    bytes.push(val);
                    continue;
                }
            }
            bytes.push(b);
        }
        String::from_utf8(bytes).unwrap_or_else(|_| input.to_string())
    }

    /// Checks if path has an image file extension.
    pub fn is_image_extension(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".webp")
            || lower.ends_with(".gif")
            || lower.ends_with(".svg")
            || lower.ends_with(".bmp")
            || lower.ends_with(".ico")
            || lower.ends_with(".tiff")
            || lower.ends_with(".tif")
            || lower.ends_with(".heic")
            || lower.ends_with(".avif")
    }

    /// Checks if a string looks like an absolute filesystem path.
    fn is_filesystem_path(&self, text: &str) -> bool {
        // Unix absolute path (/Users/..., /home/..., /tmp/...)
        if text.starts_with('/') && !text.contains('\n') && (text.contains('.') || Path::new(text).exists() || text.split('/').count() > 2) {
            return true;
        }

        // Windows drive path (C:\..., D:/...)
        if text.len() >= 3 && text.chars().nth(1) == Some(':') && (text.chars().nth(2) == Some('\\') || text.chars().nth(2) == Some('/')) {
            return true;
        }

        // Windows UNC network path (\\server\share\...)
        if text.starts_with("\\\\") && !text.contains('\n') {
            return true;
        }

        false
    }

    /// Checks if a string contains HTML structure tags.
    fn is_html_content(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        (lower.starts_with("<!doctype") || lower.starts_with("<html") || lower.starts_with("<div") || lower.starts_with("<p>") || lower.starts_with("<body"))
            || ((lower.contains("<html") || lower.contains("<div") || lower.contains("<p>") || lower.contains("<span") || lower.contains("<a href="))
                && lower.contains("</"))
    }

    /// Extracts domain from a normalized URL.
    pub fn extract_domain(&self, url_str: &str) -> Option<String> {
        let stripped = if let Some(s) = url_str.strip_prefix("https://") {
            s
        } else if let Some(s) = url_str.strip_prefix("http://") {
            s
        } else if let Some(s) = url_str.strip_prefix("ftp://") {
            s
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

    /// Infers MIME type from file extension.
    pub fn infer_mime_type_from_path(&self, path: &str) -> String {
        let lower = path.to_lowercase();
        if lower.ends_with(".png") {
            "image/png".to_string()
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            "image/jpeg".to_string()
        } else if lower.ends_with(".webp") {
            "image/webp".to_string()
        } else if lower.ends_with(".gif") {
            "image/gif".to_string()
        } else if lower.ends_with(".svg") {
            "image/svg+xml".to_string()
        } else if lower.ends_with(".pdf") {
            "application/pdf".to_string()
        } else if lower.ends_with(".json") {
            "application/json".to_string()
        } else if lower.ends_with(".md") {
            "text/markdown".to_string()
        } else if lower.ends_with(".txt") {
            "text/plain".to_string()
        } else if lower.ends_with(".html") || lower.ends_with(".htm") {
            "text/html".to_string()
        } else if lower.ends_with(".rs") {
            "text/x-rust".to_string()
        } else if lower.ends_with(".ts") || lower.ends_with(".tsx") {
            "text/typescript".to_string()
        } else if lower.ends_with(".js") || lower.ends_with(".jsx") {
            "text/javascript".to_string()
        } else if lower.ends_with(".py") {
            "text/x-python".to_string()
        } else if lower.ends_with(".zip") {
            "application/zip".to_string()
        } else if lower.ends_with(".docx") {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string()
        } else if lower.ends_with(".xlsx") {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()
        } else {
            "application/octet-stream".to_string()
        }
    }

    /// Infers human-readable title based on payload type and content.
    fn infer_title(
        &self,
        content_type: &ContentType,
        content: &str,
        filename: Option<&str>,
        domain: Option<&str>,
        window_title: Option<&str>,
        app_name: Option<&str>,
    ) -> Option<String> {
        match content_type {
            ContentType::Url => {
                if let Some(win) = window_title {
                    let cleaned = win.trim();
                    if !cleaned.is_empty() {
                        return Some(cleaned.to_string());
                    }
                }
                if let Some(dom) = domain {
                    let path_part = content
                        .split('/')
                        .filter(|p| !p.is_empty() && !p.contains(':') && !p.contains(dom))
                        .last();
                    if let Some(last_seg) = path_part {
                        let formatted = last_seg.replace('-', " ").replace('_', " ");
                        return Some(format!("{} - {}", dom, formatted));
                    }
                    return Some(dom.to_string());
                }
                Some("Web Page".to_string())
            }
            ContentType::File => {
                if let Some(fname) = filename {
                    Some(fname.to_string())
                } else {
                    Some("Dropped File".to_string())
                }
            }
            ContentType::Image => {
                if let Some(fname) = filename {
                    Some(fname.to_string())
                } else if let Some(app) = app_name {
                    Some(format!("Image from {}", app))
                } else {
                    Some("Dropped Image".to_string())
                }
            }
            ContentType::RichText => {
                if let Some(win) = window_title {
                    let cleaned = win.trim();
                    if !cleaned.is_empty() {
                        return Some(cleaned.to_string());
                    }
                }
                // Strip tags for brief text preview
                let stripped = content
                    .replace("<br>", " ")
                    .replace("<p>", " ")
                    .replace("</p>", " ");
                let first_line = stripped
                    .lines()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty())
                    .unwrap_or("Rich Document");
                let clean_snippet: String = first_line.chars().take(50).collect();
                Some(clean_snippet)
            }
            ContentType::PlainText | ContentType::Unknown | ContentType::Custom(_) => {
                if let Some(win) = window_title {
                    let cleaned = win.trim();
                    if !cleaned.is_empty() && cleaned.len() <= 60 {
                        return Some(cleaned.to_string());
                    }
                }
                let first_line = content
                    .lines()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty())
                    .unwrap_or("Text Snippet");
                let truncated: String = first_line.chars().take(60).collect();
                Some(truncated)
            }
        }
    }
}

// =============================================================================
// Unit Tests for Each Supported Payload Type
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url_http_and_https() {
        let normalizer = DragPayloadNormalizer::new();

        // 1. Standard HTTPS URL
        let payload = RawDragPayload::from_url("https://github.com/rust-lang/rust");
        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.capture_type, CaptureType::DragDrop);
        assert_eq!(event.content_type, ContentType::Url);
        assert_eq!(event.content, "https://github.com/rust-lang/rust");
        assert_eq!(event.domain().as_deref(), Some("github.com"));
        assert_eq!(event.mime_type().as_deref(), Some("text/uri-list"));

        // 2. HTTP URL
        let payload_http = RawDragPayload::from_content("http://example.com/index.html");
        let event_http = normalizer.normalize_drag_payload(&payload_http).unwrap();
        assert_eq!(event_http.content_type, ContentType::Url);
        assert_eq!(event_http.domain().as_deref(), Some("example.com"));
    }

    #[test]
    fn test_normalize_url_without_scheme_www() {
        let normalizer = DragPayloadNormalizer::new();
        let payload = RawDragPayload::from_content("www.contextsplit.dev/roadmap");
        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.content_type, ContentType::Url);
        assert_eq!(event.content, "https://www.contextsplit.dev/roadmap");
        assert_eq!(event.domain().as_deref(), Some("contextsplit.dev"));
    }

    #[test]
    fn test_normalize_plain_text() {
        let normalizer = DragPayloadNormalizer::new();
        let text = "fn main() {\n    println!(\"Hello ContextSplit!\");\n}";
        let payload = RawDragPayload::from_content(text).with_app_info(Some("VS Code".to_string()), Some("main.rs".to_string()));

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.capture_type, CaptureType::DragDrop);
        assert_eq!(event.content_type, ContentType::PlainText);
        assert_eq!(event.content, text);
        assert_eq!(event.mime_type().as_deref(), Some("text/plain"));
        assert_eq!(event.source_application.as_deref(), Some("VS Code"));
        assert_eq!(event.title().as_deref(), Some("main.rs"));
    }

    #[test]
    fn test_normalize_plain_text_with_crlf_conversion() {
        let normalizer = DragPayloadNormalizer::new();
        let raw_crlf = "Line 1\r\nLine 2\r\nLine 3";
        let payload = RawDragPayload::from_content(raw_crlf);

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.content_type, ContentType::PlainText);
        assert_eq!(event.content, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_normalize_file_local_path() {
        let normalizer = DragPayloadNormalizer::new();
        let payload = RawDragPayload::from_files(vec!["/Users/username/Documents/Report.pdf".to_string()]);

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.capture_type, CaptureType::DragDrop);
        assert_eq!(event.content_type, ContentType::File);
        assert_eq!(event.content, "/Users/username/Documents/Report.pdf");
        assert_eq!(event.filename().as_deref(), Some("Report.pdf"));
        assert_eq!(event.mime_type().as_deref(), Some("application/pdf"));
        assert_eq!(event.title().as_deref(), Some("Report.pdf"));
    }

    #[test]
    fn test_normalize_file_url_percent_encoded() {
        let normalizer = DragPayloadNormalizer::new();
        let payload = RawDragPayload::from_url("file:///Users/username/My%20Project/Spec%20Final.docx");

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.content_type, ContentType::File);
        assert_eq!(event.content, "/Users/username/My Project/Spec Final.docx");
        assert_eq!(event.filename().as_deref(), Some("Spec Final.docx"));
        assert_eq!(
            event.mime_type().as_deref(),
            Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        );
    }

    #[test]
    fn test_normalize_file_windows_path() {
        let normalizer = DragPayloadNormalizer::new();
        let payload = RawDragPayload::from_content("C:\\Users\\User\\Documents\\architecture.png");

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        // Since it's .png, it correctly detects as Image
        assert_eq!(event.content_type, ContentType::Image);
        assert_eq!(event.content, "C:\\Users\\User\\Documents\\architecture.png");
        assert_eq!(event.filename().as_deref(), Some("architecture.png"));
        assert_eq!(event.mime_type().as_deref(), Some("image/png"));
    }

    #[test]
    fn test_normalize_image_data_uri() {
        let normalizer = DragPayloadNormalizer::new();
        let data_uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let payload = RawDragPayload::from_content(data_uri);

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.capture_type, CaptureType::DragDrop);
        assert_eq!(event.content_type, ContentType::Image);
        assert_eq!(event.content, data_uri);
        assert_eq!(event.mime_type().as_deref(), Some("image/png"));
    }

    #[test]
    fn test_normalize_image_raw_bytes() {
        let normalizer = DragPayloadNormalizer::new();
        let raw_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG magic header
        let payload = RawDragPayload::from_image_bytes(raw_bytes);

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.content_type, ContentType::Image);
        assert!(event.content.starts_with("data:image/png;base64,"));
        assert_eq!(event.mime_type().as_deref(), Some("image/png"));
    }

    #[test]
    fn test_normalize_rich_text_html() {
        let normalizer = DragPayloadNormalizer::new();
        let html = "<div class=\"article\"><p>Important design decision for ContextSplit</p></div>";
        let payload = RawDragPayload::from_html(html).with_app_info(Some("Chrome".to_string()), Some("Design Doc".to_string()));

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.capture_type, CaptureType::DragDrop);
        assert_eq!(event.content_type, ContentType::RichText);
        assert_eq!(event.content, html);
        assert_eq!(event.mime_type().as_deref(), Some("text/html"));
        assert_eq!(event.title().as_deref(), Some("Design Doc"));
    }

    #[test]
    fn test_normalize_rich_text_rtf() {
        let normalizer = DragPayloadNormalizer::new();
        let rtf = "{\\rtf1\\ansi\\deff0 {\\fonttbl {\\f0 Arial;}}\\f0\\fs24 Hello RTF!}";
        let payload = RawDragPayload::from_rtf(rtf);

        let event = normalizer.normalize_drag_payload(&payload).unwrap();

        assert_eq!(event.content_type, ContentType::RichText);
        assert_eq!(event.content, rtf);
        assert_eq!(event.mime_type().as_deref(), Some("text/rtf"));
    }

    #[test]
    fn test_normalize_empty_payload_returns_error() {
        let normalizer = DragPayloadNormalizer::new();
        let empty_payload = RawDragPayload::new();

        let result = normalizer.normalize_drag_payload(&empty_payload);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CaptureError::EmptyContent);
    }

    #[test]
    fn test_normalize_platform_drag_drop_input() {
        let normalizer = DragPayloadNormalizer::new();
        let input = PlatformDragDropInput {
            paths: vec!["/home/user/project/README.md".to_string()],
            text: None,
            url: None,
            active_app: Some("Files".to_string()),
            window_title: Some("project".to_string()),
            workspace_id: Some("ws-123".to_string()),
        };

        let event = normalizer.normalize_platform_input(input, PlatformType::Linux).unwrap();

        assert_eq!(event.capture_type, CaptureType::DragDrop);
        assert_eq!(event.content_type, ContentType::File);
        assert_eq!(event.content, "/home/user/project/README.md");
        assert_eq!(event.filename().as_deref(), Some("README.md"));
        assert_eq!(event.mime_type().as_deref(), Some("text/markdown"));
        assert_eq!(event.workspace_id().as_deref(), Some("ws-123"));
        assert_eq!(event.platform, PlatformType::Linux);
    }
}

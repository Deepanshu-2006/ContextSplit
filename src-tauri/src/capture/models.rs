use serde::{Deserialize, Serialize};

/// Method or user action through which content was captured.
/// Supports standard capture types and custom extensions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureType {
    Clipboard,
    DragDrop,
    Manual,
    #[serde(untagged)]
    Custom(String),
}

impl std::fmt::Display for CaptureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clipboard => write!(f, "clipboard"),
            Self::DragDrop => write!(f, "drag_drop"),
            Self::Manual => write!(f, "manual"),
            Self::Custom(s) => write!(f, "{}", s),
        }
    }
}

impl From<&str> for CaptureType {
    fn from(s: &str) -> Self {
        match s {
            "clipboard" => Self::Clipboard,
            "drag_drop" | "drag_and_drop" => Self::DragDrop,
            "manual" => Self::Manual,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Backwards-compatibility type alias.
pub type CaptureMethod = CaptureType;

/// Target operating system platform where the capture originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformType {
    MacOS,
    Windows,
    Linux,
    Generic,
}

impl std::fmt::Display for PlatformType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MacOS => write!(f, "macos"),
            Self::Windows => write!(f, "windows"),
            Self::Linux => write!(f, "linux"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

impl From<&str> for PlatformType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "macos" | "darwin" => Self::MacOS,
            "windows" | "win32" => Self::Windows,
            "linux" => Self::Linux,
            _ => Self::Generic,
        }
    }
}

impl PlatformType {
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::MacOS
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Self::Generic
        }
    }
}

/// High-level content type of the captured item.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    PlainText,
    Url,
    Image,
    File,
    RichText,
    Unknown,
    #[serde(untagged)]
    Custom(String),
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlainText => write!(f, "plain_text"),
            Self::Url => write!(f, "url"),
            Self::Image => write!(f, "image"),
            Self::File => write!(f, "file"),
            Self::RichText => write!(f, "rich_text"),
            Self::Unknown => write!(f, "unknown"),
            Self::Custom(s) => write!(f, "{}", s),
        }
    }
}

impl From<&str> for ContentType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "plain_text" | "text" => Self::PlainText,
            "url" | "uri" => Self::Url,
            "image" | "png" | "jpeg" | "jpg" | "webp" => Self::Image,
            "file" => Self::File,
            "rich_text" | "rtf" | "html" => Self::RichText,
            "unknown" => Self::Unknown,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Backwards-compatibility type alias.
pub type ItemType = ContentType;

/// Originating application metadata and window context.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub window_title: Option<String>,
    pub process_name: Option<String>,
    pub bundle_identifier: Option<String>,
    pub url_origin: Option<String>,
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// Raw, unconverted capture payload received from a platform-specific listener or UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawCapturePayload {
    /// Raw content string (text, URL string, base64 data URI, file path, etc.)
    pub content: String,
    /// Explicit or suggested data type (e.g. "text", "url", "image", "file")
    pub data_type: Option<String>,
    /// Optional user-provided or window-derived title
    pub title: Option<String>,
    /// How the content was captured
    pub source_method: CaptureType,
    /// Originating operating system platform
    pub platform: PlatformType,
    /// Active application name at the moment of capture
    pub app_name: Option<String>,
    /// Active window title at the moment of capture
    pub window_title: Option<String>,
    /// OS bundle ID (macOS) or process name/executable (Windows)
    pub process_info: Option<String>,
    /// List of file paths (for multi-file drag-drop or file drops)
    pub file_paths: Vec<String>,
    /// Inferred or provided MIME type
    pub mime_type: Option<String>,
    /// Original URL if captured from browser
    pub original_url: Option<String>,
    /// Explicit byte size if known
    pub size_bytes: Option<usize>,
    /// Target workspace ID (if captured directly into a workspace; None for Inbox)
    pub workspace_id: Option<String>,
    /// Arbitrary platform-specific metadata
    pub raw_metadata: Option<serde_json::Value>,
}

impl RawCapturePayload {
    pub fn new(content: impl Into<String>, source_method: CaptureType) -> Self {
        Self {
            content: content.into(),
            data_type: None,
            title: None,
            source_method,
            platform: PlatformType::current(),
            app_name: None,
            window_title: None,
            process_info: None,
            file_paths: Vec::new(),
            mime_type: None,
            original_url: None,
            size_bytes: None,
            workspace_id: None,
            raw_metadata: None,
        }
    }

    pub fn with_type(mut self, data_type: impl Into<String>) -> Self {
        self.data_type = Some(data_type.into());
        self
    }

    pub fn with_app_info(mut self, app_name: Option<String>, window_title: Option<String>) -> Self {
        self.app_name = app_name;
        self.window_title = window_title;
        self
    }

    pub fn with_platform(mut self, platform: PlatformType) -> Self {
        self.platform = platform;
        self
    }
}

/// Normalized, platform-independent CaptureEvent model.
///
/// CRITICAL ARCHITECTURAL RULE:
/// CaptureEvent is strictly decoupled from any single OS, exposing a canonical schema
/// that any platform adapter can produce and that CaptureCore ingests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureEvent {
    /// Unique identifier for this capture event
    pub event_id: String,
    /// Method used to capture the content (clipboard, drag_drop, manual, custom)
    pub capture_type: CaptureType,
    /// ISO 8601 / RFC 3339 UTC capture timestamp
    pub timestamp: String,
    /// Category of captured data (plain_text, url, image, file, rich_text, unknown)
    pub content_type: ContentType,
    /// Cleaned, normalized content string
    pub content: String,
    /// Name of the originating foreground application (e.g. "Safari", "VS Code")
    pub source_application: Option<String>,
    /// Detailed metadata about the source window/process
    pub source_metadata: SourceMetadata,
    /// Platform where the capture occurred
    pub platform: PlatformType,
    /// Extensible structured metadata JSON map
    pub metadata: serde_json::Value,
}

impl CaptureEvent {
    /// Creates a new CaptureEvent with generated UUID and current UTC timestamp.
    pub fn new(
        capture_type: CaptureType,
        content_type: ContentType,
        content: impl Into<String>,
        platform: PlatformType,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            capture_type,
            timestamp: chrono::Utc::now().to_rfc3339(),
            content_type,
            content: content.into(),
            source_application: None,
            source_metadata: SourceMetadata::default(),
            platform,
            metadata: serde_json::json!({}),
        }
    }

    /// Validates the CaptureEvent against schema and data integrity constraints.
    pub fn validate(&self) -> Result<(), CaptureError> {
        // 1. Event ID validation
        if self.event_id.trim().is_empty() {
            return Err(CaptureError::ProcessingError("event_id cannot be empty".to_string()));
        }
        if self.event_id.contains('\0') {
            return Err(CaptureError::ProcessingError("event_id contains illegal null bytes".to_string()));
        }

        // 2. Timestamp validation (RFC 3339)
        if chrono::DateTime::parse_from_rfc3339(&self.timestamp).is_err() {
            return Err(CaptureError::ProcessingError(format!(
                "Invalid RFC 3339 timestamp: '{}'",
                self.timestamp
            )));
        }

        // 3. Content emptiness and null byte validation
        if self.content.trim().is_empty() && self.content_type != ContentType::Unknown {
            return Err(CaptureError::EmptyContent);
        }
        if self.content.contains('\0') {
            return Err(CaptureError::ProcessingError("content contains illegal null bytes".to_string()));
        }

        // 4. Size bounds
        if self.content.len() > 10 * 1024 * 1024 {
            return Err(CaptureError::PayloadTooLarge {
                size: self.content.len(),
                limit: 10 * 1024 * 1024,
            });
        }

        // 5. Type-specific validation
        match &self.content_type {
            ContentType::Url => {
                let trimmed = self.content.trim();
                if !trimmed.starts_with("http://")
                    && !trimmed.starts_with("https://")
                    && !trimmed.starts_with("file://")
                {
                    return Err(CaptureError::InvalidUrl(format!(
                        "URL must begin with http://, https://, or file:// (got: {})",
                        trimmed
                    )));
                }
            }
            ContentType::File => {
                let trimmed = self.content.trim();
                if trimmed.is_empty() {
                    return Err(CaptureError::InvalidFile("File path cannot be empty".to_string()));
                }
            }
            ContentType::Image => {
                let trimmed = self.content.trim();
                if trimmed.is_empty() {
                    return Err(CaptureError::InvalidImage("Image content is empty".to_string()));
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Convenience accessor for legacy item_type string ("text", "url", "image", "file").
    pub fn item_type_str(&self) -> &str {
        match &self.content_type {
            ContentType::PlainText => "text",
            ContentType::Url => "url",
            ContentType::Image => "image",
            ContentType::File => "file",
            ContentType::RichText => "text",
            ContentType::Unknown => "text",
            ContentType::Custom(s) => s.as_str(),
        }
    }

    /// Convenience accessor for title stored in metadata or source_metadata.
    pub fn title(&self) -> Option<String> {
        self.metadata
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| self.source_metadata.window_title.clone())
    }

    /// Convenience accessor for target workspace_id.
    pub fn workspace_id(&self) -> Option<String> {
        self.metadata
            .get("workspace_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Convenience accessor for session_id.
    pub fn session_id(&self) -> Option<String> {
        self.metadata
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Convenience accessor for domain.
    pub fn domain(&self) -> Option<String> {
        self.metadata
            .get("domain")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Convenience accessor for filename.
    pub fn filename(&self) -> Option<String> {
        self.metadata
            .get("filename")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Convenience accessor for MIME type.
    pub fn mime_type(&self) -> Option<String> {
        self.metadata
            .get("mime_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Convenience accessor for size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.metadata
            .get("size")
            .and_then(|v| v.as_u64())
            .map(|s| s as usize)
            .unwrap_or(self.content.len())
    }
}

/// Backwards compatibility alias.
pub type NormalizedCaptureEvent = CaptureEvent;

/// Result returned after successful processing by CaptureCore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureResult {
    /// The persisted database Item
    pub item: crate::db::models::Item,
    /// Number of ContextSignals generated and recorded
    pub signals_emitted: usize,
    /// Active session ID associated with the item
    pub session_id: Option<String>,
    /// Initial semantic processing status ("pending", "processing", or "ready")
    pub semantic_status: String,
    /// Whether this capture was a duplicate of recent identical content
    pub is_duplicate: bool,
}

/// Strongly typed errors encountered across the capture pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureError {
    /// Content is completely empty or consists only of whitespace
    EmptyContent,
    /// The provided URL syntax is invalid or unsupported
    InvalidUrl(String),
    /// File path does not exist, is inaccessible, or contains illegal characters
    InvalidFile(String),
    /// Image data is corrupted or unsupported format
    InvalidImage(String),
    /// Payload exceeds maximum allowable size limit
    PayloadTooLarge { size: usize, limit: usize },
    /// Database operation failed during item or signal creation
    DatabaseError(String),
    /// Context engine processing failed
    ContextEngineError(String),
    /// Platform-specific conversion failure
    PlatformError(String),
    /// Capture was rejected by privacy filter (sensitive content, excluded app, paused monitoring)
    PrivacyRejected(String),
    /// General capture processing failure
    ProcessingError(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyContent => write!(f, "Capture content cannot be empty or whitespace-only"),
            Self::InvalidUrl(msg) => write!(f, "Invalid URL: {}", msg),
            Self::InvalidFile(msg) => write!(f, "Invalid file: {}", msg),
            Self::InvalidImage(msg) => write!(f, "Invalid image: {}", msg),
            Self::PayloadTooLarge { size, limit } => {
                write!(f, "Payload size ({} bytes) exceeds limit ({} bytes)", size, limit)
            }
            Self::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            Self::ContextEngineError(msg) => write!(f, "Context Engine error: {}", msg),
            Self::PlatformError(msg) => write!(f, "Platform capture error: {}", msg),
            Self::PrivacyRejected(reason) => write!(f, "Capture rejected by privacy filter: {}", reason),
            Self::ProcessingError(msg) => write!(f, "Processing error: {}", msg),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Platform-specific clipboard event input before abstraction.
#[derive(Debug, Clone)]
pub struct PlatformClipboardInput {
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub image_dimensions: Option<(u32, u32)>,
    pub file_urls: Vec<String>,
    pub active_app: Option<String>,
    pub window_title: Option<String>,
}

/// Platform-specific drag-and-drop input before abstraction.
#[derive(Debug, Clone)]
pub struct PlatformDragDropInput {
    pub paths: Vec<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub active_app: Option<String>,
    pub window_title: Option<String>,
    pub workspace_id: Option<String>,
}

/// Platform-specific manual capture input.
#[derive(Debug, Clone)]
pub struct PlatformManualInput {
    pub content: String,
    pub data_type: Option<String>,
    pub title: Option<String>,
    pub workspace_id: Option<String>,
    pub active_app: Option<String>,
    pub window_title: Option<String>,
}

/// Active window context snapshot from platform queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformWindowContext {
    pub app_name: String,
    pub window_title: Option<String>,
    pub process_id: Option<u32>,
    pub bundle_identifier: Option<String>,
}

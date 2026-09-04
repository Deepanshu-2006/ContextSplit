use serde::{Deserialize, Serialize};

/// Method or user action through which content was captured.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    Clipboard,
    DragAndDrop,
    Manual,
    FileWatch,
    HotKey,
    Api,
    Other(String),
}

impl std::fmt::Display for CaptureMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clipboard => write!(f, "clipboard"),
            Self::DragAndDrop => write!(f, "drag_drop"),
            Self::Manual => write!(f, "manual"),
            Self::FileWatch => write!(f, "file_watch"),
            Self::HotKey => write!(f, "hotkey"),
            Self::Api => write!(f, "api"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

impl From<&str> for CaptureMethod {
    fn from(s: &str) -> Self {
        match s {
            "clipboard" => Self::Clipboard,
            "drag_drop" | "drag_and_drop" => Self::DragAndDrop,
            "manual" => Self::Manual,
            "file_watch" => Self::FileWatch,
            "hotkey" => Self::HotKey,
            "api" => Self::Api,
            other => Self::Other(other.to_string()),
        }
    }
}

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

/// High-level data category of the captured item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Text,
    Url,
    Image,
    File,
    Code,
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Url => write!(f, "url"),
            Self::Image => write!(f, "image"),
            Self::File => write!(f, "file"),
            Self::Code => write!(f, "code"),
        }
    }
}

impl From<&str> for ItemType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "url" => Self::Url,
            "image" => Self::Image,
            "file" => Self::File,
            "code" => Self::Code,
            _ => Self::Text,
        }
    }
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
    pub source_method: CaptureMethod,
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
    pub fn new(content: impl Into<String>, source_method: CaptureMethod) -> Self {
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

/// Normalized, validated capture event ready to be saved and ingested into Context Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedCaptureEvent {
    /// Normalized data category ("text", "url", "image", "file", "code")
    pub item_type: String,
    /// Cleaned, normalized content
    pub content: String,
    /// Normalized title (inferred if absent)
    pub title: Option<String>,
    /// Source capture method
    pub source_method: CaptureMethod,
    /// Platform where captured
    pub platform: PlatformType,
    /// Active application name
    pub app_name: Option<String>,
    /// Active window title
    pub window_title: Option<String>,
    /// Normalized domain (for URLs)
    pub domain: Option<String>,
    /// Normalized filename (for files)
    pub filename: Option<String>,
    /// Normalized MIME type
    pub mime_type: Option<String>,
    /// Canonical source URL
    pub original_url: Option<String>,
    /// Content or file size in bytes
    pub size_bytes: usize,
    /// Target workspace (None for Inbox)
    pub workspace_id: Option<String>,
    /// Associated ContextSession ID
    pub session_id: Option<String>,
    /// ISO 8601 / RFC 3339 capture timestamp
    pub captured_at: String,
    /// Full structured metadata JSON value
    pub metadata: serde_json::Value,
}

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
    /// General capture processing failure
    ProcessingError(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyContent => write!(f, "Capture content cannot be empty or whitespace-only"),
            Self::InvalidUrl(msg) => write!(f, "Invalid URL in capture: {}", msg),
            Self::InvalidFile(msg) => write!(f, "Invalid file in capture: {}", msg),
            Self::InvalidImage(msg) => write!(f, "Invalid image data in capture: {}", msg),
            Self::PayloadTooLarge { size, limit } => {
                write!(f, "Capture payload size ({} bytes) exceeds limit ({} bytes)", size, limit)
            }
            Self::DatabaseError(msg) => write!(f, "Database error during capture: {}", msg),
            Self::ContextEngineError(msg) => write!(f, "Context engine error during capture: {}", msg),
            Self::PlatformError(msg) => write!(f, "Platform capture adapter error: {}", msg),
            Self::ProcessingError(msg) => write!(f, "Capture processing error: {}", msg),
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

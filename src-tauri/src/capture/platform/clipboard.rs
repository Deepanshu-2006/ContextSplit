use serde_json::json;
use sha2::{Digest, Sha256};

use crate::capture::models::{
    CaptureError, CaptureEvent, CaptureType, ContentType, PlatformType, SourceMetadata,
};
use crate::capture::normalizer::CaptureNormalizer;
use crate::capture::privacy::ClipboardPrivacyFilter;
use crate::db::settings::PrivacySettings;

/// Common interface for OS-specific clipboard capture adapters.
pub trait ClipboardCaptureAdapter: Send + Sync {
    /// Returns the target platform type.
    fn platform(&self) -> PlatformType;

    /// Checks if the clipboard adapter monitoring is enabled.
    fn is_enabled(&self) -> bool;

    /// Enables or disables clipboard capture monitoring.
    fn set_enabled(&mut self, enabled: bool);

    /// Updates active privacy settings.
    fn set_privacy_settings(&mut self, settings: PrivacySettings);

    /// Resets the internal duplicate cache.
    fn reset_cache(&mut self);

    /// Processes raw clipboard content change and produces a normalized `CaptureEvent`.
    /// Returns `Ok(Some(CaptureEvent))` on meaningful change, or `Ok(None)` if content is unchanged / empty / disabled.
    fn on_clipboard_change(
        &mut self,
        text: Option<String>,
        active_app: Option<String>,
        window_title: Option<String>,
    ) -> Result<Option<CaptureEvent>, CaptureError>;
}

/// macOS platform clipboard adapter.
/// Emits normalized `CaptureEvent` instances with macOS platform tagging and NSPasteboard normalization.
#[derive(Debug, Clone)]
pub struct MacOSClipboardAdapter {
    last_hash: Option<String>,
    enabled: bool,
    privacy_settings: PrivacySettings,
    pub privacy_filter: ClipboardPrivacyFilter,
    normalizer: CaptureNormalizer,
}

impl Default for MacOSClipboardAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOSClipboardAdapter {
    pub fn new() -> Self {
        Self {
            last_hash: None,
            enabled: true,
            privacy_settings: PrivacySettings::default(),
            privacy_filter: ClipboardPrivacyFilter::new(),
            normalizer: CaptureNormalizer::new(),
        }
    }

    pub fn with_privacy_settings(mut self, settings: PrivacySettings) -> Self {
        self.privacy_filter = self.privacy_filter.with_privacy_settings(&settings);
        self.privacy_settings = settings;
        self
    }

    pub fn with_privacy_filter(mut self, filter: ClipboardPrivacyFilter) -> Self {
        self.privacy_filter = filter;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn compute_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.privacy_filter.is_enabled() && !self.privacy_settings.capture_paused
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.privacy_filter.set_enabled(enabled);
    }

    pub fn is_paused(&self) -> bool {
        self.privacy_filter.is_paused() || self.privacy_settings.capture_paused
    }

    pub fn pause_for(&mut self, duration: std::time::Duration) {
        self.privacy_filter.pause_for(duration);
    }

    pub fn pause_indefinitely(&mut self) {
        self.privacy_filter.pause_indefinitely();
    }

    pub fn resume(&mut self) {
        self.privacy_filter.resume();
    }

    pub fn set_privacy_settings(&mut self, settings: PrivacySettings) {
        self.privacy_filter = self.privacy_filter.clone().with_privacy_settings(&settings);
        self.privacy_settings = settings;
    }

    pub fn reset_cache(&mut self) {
        self.last_hash = None;
    }

    pub fn on_clipboard_change(
        &mut self,
        text: Option<String>,
        active_app: Option<String>,
        window_title: Option<String>,
    ) -> Result<Option<CaptureEvent>, CaptureError> {
        ClipboardCaptureAdapter::on_clipboard_change(self, text, active_app, window_title)
    }
}

impl ClipboardCaptureAdapter for MacOSClipboardAdapter {
    fn platform(&self) -> PlatformType {
        PlatformType::MacOS
    }

    fn is_enabled(&self) -> bool {
        self.enabled && self.privacy_filter.is_enabled() && !self.privacy_settings.capture_paused
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.privacy_filter.set_enabled(enabled);
    }

    fn set_privacy_settings(&mut self, settings: PrivacySettings) {
        self.privacy_filter = self.privacy_filter.clone().with_privacy_settings(&settings);
        self.privacy_settings = settings;
    }

    fn reset_cache(&mut self) {
        self.last_hash = None;
    }

    fn on_clipboard_change(
        &mut self,
        text: Option<String>,
        active_app: Option<String>,
        window_title: Option<String>,
    ) -> Result<Option<CaptureEvent>, CaptureError> {
        if !self.enabled || !self.privacy_filter.is_enabled() || self.privacy_filter.is_paused() || self.privacy_settings.capture_paused {
            return Ok(None);
        }

        let raw_text = match text {
            Some(t) => t,
            None => return Ok(None),
        };

        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        // Privacy Filter Check: reject sensitive content or excluded apps/domains before capturing
        if self.privacy_filter.filter_clipboard(trimmed, active_app.as_deref(), window_title.as_deref()).is_rejected() {
            return Ok(None);
        }

        let hash = Self::compute_hash(trimmed);
        if let Some(ref last) = self.last_hash {
            if last == &hash {
                // Avoid repeatedly capturing identical content
                return Ok(None);
            }
        }
        self.last_hash = Some(hash);

        // Content type detection (text vs URL)
        let is_url = (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            && trimmed.contains('.')
            && !trimmed.contains('\n')
            && !trimmed.contains('\r');

        let content_type = if is_url {
            ContentType::Url
        } else {
            ContentType::PlainText
        };

        let timestamp = chrono::Utc::now().to_rfc3339();

        // Respect privacy settings
        let source_app = if self.privacy_settings.enable_active_app {
            active_app
        } else {
            None
        };

        let win_title = if self.privacy_settings.enable_window_title {
            window_title
        } else {
            None
        };

        let mut meta_map = serde_json::Map::new();
        meta_map.insert("source".to_string(), json!("clipboard"));
        meta_map.insert("platform".to_string(), json!("macos"));
        meta_map.insert("content_type".to_string(), json!(content_type.to_string()));
        meta_map.insert("size".to_string(), json!(trimmed.len()));

        if let Some(ref app) = source_app {
            meta_map.insert("app_name".to_string(), json!(app));
        }
        if let Some(ref win) = win_title {
            meta_map.insert("window_title".to_string(), json!(win));
        }

        if content_type == ContentType::Url {
            if self.privacy_settings.enable_browser_domain {
                if let Some(dom) = self.normalizer.extract_domain(trimmed) {
                    meta_map.insert("domain".to_string(), json!(dom));
                }
            }
            meta_map.insert("mime_type".to_string(), json!("text/uri-list"));
        } else {
            meta_map.insert("mime_type".to_string(), json!("text/plain"));
        }

        let source_metadata = SourceMetadata {
            window_title: win_title,
            process_name: source_app.clone(),
            bundle_identifier: None,
            url_origin: if content_type == ContentType::Url {
                Some(trimmed.to_string())
            } else {
                None
            },
            extra: json!({}),
        };

        let event = CaptureEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            capture_type: CaptureType::Clipboard,
            timestamp,
            content_type,
            content: trimmed.to_string(),
            source_application: source_app,
            source_metadata,
            platform: PlatformType::MacOS,
            metadata: serde_json::Value::Object(meta_map),
        };

        Ok(Some(event))
    }
}

/// Windows platform clipboard adapter.
/// Emits normalized `CaptureEvent` instances with Windows platform tagging and Win32 format normalization.
#[derive(Debug, Clone)]
pub struct WindowsClipboardAdapter {
    last_hash: Option<String>,
    enabled: bool,
    privacy_settings: PrivacySettings,
    pub privacy_filter: ClipboardPrivacyFilter,
    normalizer: CaptureNormalizer,
}

impl Default for WindowsClipboardAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsClipboardAdapter {
    pub fn new() -> Self {
        Self {
            last_hash: None,
            enabled: true,
            privacy_settings: PrivacySettings::default(),
            privacy_filter: ClipboardPrivacyFilter::new(),
            normalizer: CaptureNormalizer::new(),
        }
    }

    pub fn with_privacy_settings(mut self, settings: PrivacySettings) -> Self {
        self.privacy_filter = self.privacy_filter.with_privacy_settings(&settings);
        self.privacy_settings = settings;
        self
    }

    pub fn with_privacy_filter(mut self, filter: ClipboardPrivacyFilter) -> Self {
        self.privacy_filter = filter;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn compute_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.privacy_filter.is_enabled() && !self.privacy_settings.capture_paused
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.privacy_filter.set_enabled(enabled);
    }

    pub fn is_paused(&self) -> bool {
        self.privacy_filter.is_paused() || self.privacy_settings.capture_paused
    }

    pub fn pause_for(&mut self, duration: std::time::Duration) {
        self.privacy_filter.pause_for(duration);
    }

    pub fn pause_indefinitely(&mut self) {
        self.privacy_filter.pause_indefinitely();
    }

    pub fn resume(&mut self) {
        self.privacy_filter.resume();
    }

    pub fn set_privacy_settings(&mut self, settings: PrivacySettings) {
        self.privacy_filter = self.privacy_filter.clone().with_privacy_settings(&settings);
        self.privacy_settings = settings;
    }

    pub fn reset_cache(&mut self) {
        self.last_hash = None;
    }

    pub fn on_clipboard_change(
        &mut self,
        text: Option<String>,
        active_app: Option<String>,
        window_title: Option<String>,
    ) -> Result<Option<CaptureEvent>, CaptureError> {
        ClipboardCaptureAdapter::on_clipboard_change(self, text, active_app, window_title)
    }
}

impl ClipboardCaptureAdapter for WindowsClipboardAdapter {
    fn platform(&self) -> PlatformType {
        PlatformType::Windows
    }

    fn is_enabled(&self) -> bool {
        self.enabled && self.privacy_filter.is_enabled() && !self.privacy_settings.capture_paused
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.privacy_filter.set_enabled(enabled);
    }

    fn set_privacy_settings(&mut self, settings: PrivacySettings) {
        self.privacy_filter = self.privacy_filter.clone().with_privacy_settings(&settings);
        self.privacy_settings = settings;
    }

    fn reset_cache(&mut self) {
        self.last_hash = None;
    }

    fn on_clipboard_change(
        &mut self,
        text: Option<String>,
        active_app: Option<String>,
        window_title: Option<String>,
    ) -> Result<Option<CaptureEvent>, CaptureError> {
        if !self.enabled || !self.privacy_filter.is_enabled() || self.privacy_filter.is_paused() || self.privacy_settings.capture_paused {
            return Ok(None);
        }

        let raw_text = match text {
            Some(t) => t,
            None => return Ok(None),
        };

        // Normalize Windows CRLF -> LF
        let normalized = raw_text.replace("\r\n", "\n");
        let trimmed = normalized.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        // Privacy Filter Check: reject sensitive content or excluded apps/domains before capturing
        if self.privacy_filter.filter_clipboard(trimmed, active_app.as_deref(), window_title.as_deref()).is_rejected() {
            return Ok(None);
        }

        let hash = Self::compute_hash(trimmed);
        if let Some(ref last) = self.last_hash {
            if last == &hash {
                // Avoid repeatedly capturing identical content
                return Ok(None);
            }
        }
        self.last_hash = Some(hash);

        // Content type detection (text vs URL)
        let is_url = (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            && trimmed.contains('.')
            && !trimmed.contains('\n');

        let content_type = if is_url {
            ContentType::Url
        } else {
            ContentType::PlainText
        };

        let timestamp = chrono::Utc::now().to_rfc3339();

        // Respect privacy settings
        let source_app = if self.privacy_settings.enable_active_app {
            active_app
        } else {
            None
        };

        let win_title = if self.privacy_settings.enable_window_title {
            window_title
        } else {
            None
        };

        let mut meta_map = serde_json::Map::new();
        meta_map.insert("source".to_string(), json!("clipboard"));
        meta_map.insert("platform".to_string(), json!("windows"));
        meta_map.insert("content_type".to_string(), json!(content_type.to_string()));
        meta_map.insert("size".to_string(), json!(trimmed.len()));

        if let Some(ref app) = source_app {
            meta_map.insert("app_name".to_string(), json!(app));
        }
        if let Some(ref win) = win_title {
            meta_map.insert("window_title".to_string(), json!(win));
        }

        if content_type == ContentType::Url {
            if self.privacy_settings.enable_browser_domain {
                if let Some(dom) = self.normalizer.extract_domain(trimmed) {
                    meta_map.insert("domain".to_string(), json!(dom));
                }
            }
            meta_map.insert("mime_type".to_string(), json!("text/uri-list"));
        } else {
            meta_map.insert("mime_type".to_string(), json!("text/plain"));
        }

        let source_metadata = SourceMetadata {
            window_title: win_title,
            process_name: source_app.clone(),
            bundle_identifier: None,
            url_origin: if content_type == ContentType::Url {
                Some(trimmed.to_string())
            } else {
                None
            },
            extra: json!({}),
        };

        let event = CaptureEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            capture_type: CaptureType::Clipboard,
            timestamp,
            content_type,
            content: trimmed.to_string(),
            source_application: source_app,
            source_metadata,
            platform: PlatformType::Windows,
            metadata: serde_json::Value::Object(meta_map),
        };

        Ok(Some(event))
    }
}

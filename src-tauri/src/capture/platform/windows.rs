use base64::{engine::general_purpose, Engine as _};
use image::{ImageFormat, RgbaImage};
use std::io::Cursor;

use super::PlatformCaptureAdapter;
use crate::capture::models::{
    CaptureError, CaptureMethod, PlatformClipboardInput, PlatformDragDropInput,
    PlatformManualInput, PlatformType, PlatformWindowContext, RawCapturePayload,
};

/// Windows platform capture adapter.
/// Converts Windows clipboard formats (CF_UNICODETEXT, CF_HDROP, CF_DIB) and Explorer drag/drop events into `RawCapturePayload`.
#[derive(Debug, Clone, Default)]
pub struct WindowsCaptureAdapter;

impl WindowsCaptureAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Normalizes Windows path separators and removes quotes if dragged as a quoted string.
    pub fn clean_windows_path(path: &str) -> String {
        let trimmed = path.trim().trim_matches('"');
        trimmed.replace('/', "\\")
    }
}

impl PlatformCaptureAdapter for WindowsCaptureAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Windows
    }

    fn convert_clipboard_input(
        &self,
        input: PlatformClipboardInput,
    ) -> Result<RawCapturePayload, CaptureError> {
        let mut payload = RawCapturePayload::new("", CaptureMethod::Clipboard)
            .with_platform(PlatformType::Windows);

        payload.app_name = input.active_app;
        payload.window_title = input.window_title;

        // 1. Check for Windows CF_HDROP file list
        if !input.file_urls.is_empty() {
            let cleaned_paths: Vec<String> = input
                .file_urls
                .iter()
                .map(|p| Self::clean_windows_path(p))
                .collect();

            payload.content = cleaned_paths[0].clone();
            payload.file_paths = cleaned_paths;
            payload.data_type = Some("file".to_string());
            return Ok(payload);
        }

        // 2. Check for Windows CF_DIB image bytes
        if let Some(bytes) = input.image_bytes {
            let (width, height) = input.image_dimensions.unwrap_or((0, 0));
            if width > 0 && height > 0 {
                if let Some(rgba) = RgbaImage::from_raw(width, height, bytes) {
                    let mut buf = Cursor::new(Vec::new());
                    if rgba.write_to(&mut buf, ImageFormat::Png).is_ok() {
                        let b64 = general_purpose::STANDARD.encode(buf.into_inner());
                        payload.content = format!("data:image/png;base64,{}", b64);
                        payload.data_type = Some("image".to_string());
                        payload.mime_type = Some("image/png".to_string());
                        return Ok(payload);
                    }
                }
            } else if !bytes.is_empty() {
                let b64 = general_purpose::STANDARD.encode(&bytes);
                payload.content = format!("data:image/png;base64,{}", b64);
                payload.data_type = Some("image".to_string());
                payload.mime_type = Some("image/png".to_string());
                return Ok(payload);
            }
        }

        // 3. Check for text or URLs (CF_UNICODETEXT)
        if let Some(text) = input.text {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(CaptureError::EmptyContent);
            }

            payload.content = text.clone();
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                payload.data_type = Some("url".to_string());
                payload.original_url = Some(trimmed.to_string());
            } else if trimmed.len() > 3 && trimmed.chars().nth(1) == Some(':') && (trimmed.chars().nth(2) == Some('\\') || trimmed.chars().nth(2) == Some('/')) {
                let cleaned = Self::clean_windows_path(trimmed);
                payload.content = cleaned.clone();
                payload.file_paths = vec![cleaned];
                payload.data_type = Some("file".to_string());
            } else {
                payload.data_type = Some("text".to_string());
            }
            return Ok(payload);
        }

        Err(CaptureError::EmptyContent)
    }

    fn convert_drag_drop_input(
        &self,
        input: PlatformDragDropInput,
    ) -> Result<RawCapturePayload, CaptureError> {
        let mut payload = RawCapturePayload::new("", CaptureMethod::DragAndDrop)
            .with_platform(PlatformType::Windows);

        payload.app_name = input.active_app;
        payload.window_title = input.window_title;
        payload.workspace_id = input.workspace_id;

        // 1. Files dropped from Windows Explorer
        if !input.paths.is_empty() {
            let cleaned: Vec<String> = input
                .paths
                .iter()
                .map(|p| Self::clean_windows_path(p))
                .collect();

            payload.content = cleaned[0].clone();
            payload.file_paths = cleaned;
            payload.data_type = Some("file".to_string());
            return Ok(payload);
        }

        // 2. URL dropped
        if let Some(url) = input.url {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                payload.content = trimmed.to_string();
                payload.data_type = Some("url".to_string());
                payload.original_url = Some(trimmed.to_string());
                return Ok(payload);
            }
        }

        // 3. Text snippet dropped
        if let Some(text) = input.text {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(CaptureError::EmptyContent);
            }

            payload.content = text;
            payload.data_type = Some("text".to_string());
            return Ok(payload);
        }

        Err(CaptureError::EmptyContent)
    }

    fn convert_manual_input(
        &self,
        input: PlatformManualInput,
    ) -> Result<RawCapturePayload, CaptureError> {
        let trimmed = input.content.trim();
        if trimmed.is_empty() {
            return Err(CaptureError::EmptyContent);
        }

        let mut payload = RawCapturePayload::new(input.content, CaptureMethod::Manual)
            .with_platform(PlatformType::Windows);

        payload.data_type = input.data_type;
        payload.title = input.title;
        payload.workspace_id = input.workspace_id;
        payload.app_name = input.active_app;
        payload.window_title = input.window_title;

        Ok(payload)
    }

    fn get_active_window_context(&self) -> Option<PlatformWindowContext> {
        // In live Windows environments, this can query GetForegroundWindow / GetWindowText / GetWindowThreadProcessId
        None
    }
}

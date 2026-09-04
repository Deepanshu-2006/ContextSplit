use base64::{engine::general_purpose, Engine as _};
use image::{ImageFormat, RgbaImage};
use std::io::Cursor;

use super::PlatformCaptureAdapter;
use crate::capture::models::{
    CaptureError, CaptureMethod, CaptureType, PlatformClipboardInput, PlatformDragDropInput,
    PlatformManualInput, PlatformType, PlatformWindowContext, RawCapturePayload,
};

/// macOS platform capture adapter.
/// Converts macOS pasteboard types (NSPasteboard) and Finder drag/drop events into `RawCapturePayload`.
#[derive(Debug, Clone, Default)]
pub struct MacOSCaptureAdapter;

impl MacOSCaptureAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Cleans macOS `file://` URL strings into local filesystem paths.
    pub fn clean_file_url(file_url: &str) -> String {
        let trimmed = file_url.trim();
        if let Some(stripped) = trimmed.strip_prefix("file://localhost") {
            percent_decode_str(stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("file://") {
            percent_decode_str(stripped)
        } else {
            percent_decode_str(trimmed)
        }
    }
}

/// Simple percent decoder for macOS file URLs.
fn percent_decode_str(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(b'0');
            let h2 = chars.next().unwrap_or(b'0');
            if let Ok(val) = u8::from_str_radix(
                &format!("{}{}", h1 as char, h2 as char),
                16,
            ) {
                bytes.push(val);
                continue;
            }
        }
        bytes.push(b);
    }
    String::from_utf8(bytes).unwrap_or_else(|_| input.to_string())
}

impl PlatformCaptureAdapter for MacOSCaptureAdapter {
    fn platform_type(&self) -> PlatformType {
        PlatformType::MacOS
    }

    fn convert_clipboard_input(
        &self,
        input: PlatformClipboardInput,
    ) -> Result<RawCapturePayload, CaptureError> {
        let mut payload = RawCapturePayload::new("", CaptureMethod::Clipboard)
            .with_platform(PlatformType::MacOS);

        payload.app_name = input.active_app;
        payload.window_title = input.window_title;

        // 1. Check for file URLs from NSPasteboard (public.file-url)
        if !input.file_urls.is_empty() {
            let cleaned_paths: Vec<String> = input
                .file_urls
                .iter()
                .map(|u| Self::clean_file_url(u))
                .collect();

            payload.content = cleaned_paths[0].clone();
            payload.file_paths = cleaned_paths;
            payload.data_type = Some("file".to_string());
            return Ok(payload);
        }

        // 2. Check for image bytes (public.png / public.tiff)
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

        // 3. Check for text or URLs (public.utf8-plain-text / public.url)
        if let Some(text) = input.text {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(CaptureError::EmptyContent);
            }

            payload.content = text.clone();
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                payload.data_type = Some("url".to_string());
                payload.original_url = Some(trimmed.to_string());
            } else if trimmed.starts_with("file://") {
                let cleaned = Self::clean_file_url(trimmed);
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
        let mut payload = RawCapturePayload::new("", CaptureType::DragDrop)
            .with_platform(PlatformType::MacOS);

        payload.app_name = input.active_app;
        payload.window_title = input.window_title;
        payload.workspace_id = input.workspace_id;

        // 1. Files dropped from Finder
        if !input.paths.is_empty() {
            let cleaned: Vec<String> = input
                .paths
                .iter()
                .map(|p| Self::clean_file_url(p))
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
                if trimmed.starts_with("file://") {
                    let cleaned = Self::clean_file_url(trimmed);
                    payload.content = cleaned.clone();
                    payload.file_paths = vec![cleaned];
                    payload.data_type = Some("file".to_string());
                } else {
                    payload.content = trimmed.to_string();
                    payload.data_type = Some("url".to_string());
                    payload.original_url = Some(trimmed.to_string());
                }
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
            .with_platform(PlatformType::MacOS);

        payload.data_type = input.data_type;
        payload.title = input.title;
        payload.workspace_id = input.workspace_id;
        payload.app_name = input.active_app;
        payload.window_title = input.window_title;

        Ok(payload)
    }

    fn get_active_window_context(&self) -> Option<PlatformWindowContext> {
        // In live macOS environments, this can query NSWorkspace.sharedWorkspace.frontmostApplication
        // For portable abstraction, returns structure when queried
        None
    }
}

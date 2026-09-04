use super::PlatformCaptureAdapter;
use crate::capture::models::{
    CaptureError, CaptureMethod, PlatformClipboardInput, PlatformDragDropInput,
    PlatformManualInput, PlatformType, PlatformWindowContext, RawCapturePayload,
};

/// Mock platform capture adapter for automated testing across any OS.
#[derive(Debug, Clone)]
pub struct MockCaptureAdapter {
    pub platform: PlatformType,
    pub simulated_window_context: Option<PlatformWindowContext>,
}

impl MockCaptureAdapter {
    pub fn new(platform: PlatformType) -> Self {
        Self {
            platform,
            simulated_window_context: None,
        }
    }

    pub fn with_context(mut self, context: PlatformWindowContext) -> Self {
        self.simulated_window_context = Some(context);
        self
    }
}

impl Default for MockCaptureAdapter {
    fn default() -> Self {
        Self::new(PlatformType::Generic)
    }
}

impl PlatformCaptureAdapter for MockCaptureAdapter {
    fn platform_type(&self) -> PlatformType {
        self.platform
    }

    fn convert_clipboard_input(
        &self,
        input: PlatformClipboardInput,
    ) -> Result<RawCapturePayload, CaptureError> {
        let mut payload = RawCapturePayload::new("", CaptureMethod::Clipboard)
            .with_platform(self.platform);

        payload.app_name = input.active_app.or_else(|| self.simulated_window_context.as_ref().map(|c| c.app_name.clone()));
        payload.window_title = input.window_title.or_else(|| self.simulated_window_context.as_ref().and_then(|c| c.window_title.clone()));

        if !input.file_urls.is_empty() {
            payload.content = input.file_urls[0].clone();
            payload.file_paths = input.file_urls;
            payload.data_type = Some("file".to_string());
            return Ok(payload);
        }

        if let Some(bytes) = input.image_bytes {
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            payload.content = format!("data:image/png;base64,{}", b64);
            payload.data_type = Some("image".to_string());
            payload.mime_type = Some("image/png".to_string());
            return Ok(payload);
        }

        if let Some(text) = input.text {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(CaptureError::EmptyContent);
            }

            payload.content = text.clone();
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                payload.data_type = Some("url".to_string());
                payload.original_url = Some(trimmed.to_string());
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
            .with_platform(self.platform);

        payload.app_name = input.active_app.or_else(|| self.simulated_window_context.as_ref().map(|c| c.app_name.clone()));
        payload.window_title = input.window_title.or_else(|| self.simulated_window_context.as_ref().and_then(|c| c.window_title.clone()));
        payload.workspace_id = input.workspace_id;

        if !input.paths.is_empty() {
            payload.content = input.paths[0].clone();
            payload.file_paths = input.paths;
            payload.data_type = Some("file".to_string());
            return Ok(payload);
        }

        if let Some(url) = input.url {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                payload.content = trimmed.to_string();
                payload.data_type = Some("url".to_string());
                payload.original_url = Some(trimmed.to_string());
                return Ok(payload);
            }
        }

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
            .with_platform(self.platform);

        payload.data_type = input.data_type;
        payload.title = input.title;
        payload.workspace_id = input.workspace_id;
        payload.app_name = input.active_app.or_else(|| self.simulated_window_context.as_ref().map(|c| c.app_name.clone()));
        payload.window_title = input.window_title.or_else(|| self.simulated_window_context.as_ref().and_then(|c| c.window_title.clone()));

        Ok(payload)
    }

    fn get_active_window_context(&self) -> Option<PlatformWindowContext> {
        self.simulated_window_context.clone()
    }
}

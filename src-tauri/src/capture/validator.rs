use super::models::{CaptureError, RawCapturePayload};

/// Standard validation thresholds.
pub const MAX_TEXT_PAYLOAD_BYTES: usize = 10 * 1024 * 1024; // 10 MB
pub const MAX_FILE_SIZE_BYTES: usize = 100 * 1024 * 1024; // 100 MB

/// Platform-independent validator for raw capture payloads.
#[derive(Debug, Clone, Default)]
pub struct CaptureValidator {
    pub max_text_bytes: usize,
    pub max_file_bytes: usize,
}

impl CaptureValidator {
    pub fn new() -> Self {
        Self {
            max_text_bytes: MAX_TEXT_PAYLOAD_BYTES,
            max_file_bytes: MAX_FILE_SIZE_BYTES,
        }
    }

    pub fn with_limits(max_text_bytes: usize, max_file_bytes: usize) -> Self {
        Self {
            max_text_bytes,
            max_file_bytes,
        }
    }

    /// Validates a raw capture payload before normalization and storage.
    pub fn validate(&self, payload: &RawCapturePayload) -> Result<(), CaptureError> {
        // 1. Check for empty payload
        let has_content = !payload.content.trim().is_empty();
        let has_files = !payload.file_paths.is_empty();

        if !has_content && !has_files {
            return Err(CaptureError::EmptyContent);
        }

        // 2. Check content size bounds
        let content_len = payload.content.len();
        if content_len > self.max_text_bytes {
            return Err(CaptureError::PayloadTooLarge {
                size: content_len,
                limit: self.max_text_bytes,
            });
        }

        if let Some(explicit_size) = payload.size_bytes {
            if explicit_size > self.max_file_bytes {
                return Err(CaptureError::PayloadTooLarge {
                    size: explicit_size,
                    limit: self.max_file_bytes,
                });
            }
        }

        // 3. Check for illegal null bytes or control characters in metadata fields
        if payload.content.contains('\0') {
            return Err(CaptureError::ProcessingError("Content contains null bytes".to_string()));
        }

        if let Some(title) = &payload.title {
            if title.contains('\0') {
                return Err(CaptureError::ProcessingError("Title contains null bytes".to_string()));
            }
        }

        if let Some(ws_id) = &payload.workspace_id {
            if ws_id.trim().is_empty() || ws_id.contains('\0') {
                return Err(CaptureError::ProcessingError("Invalid workspace_id in payload".to_string()));
            }
        }

        // 4. Validate URL formatting if explicitly typed as URL
        if let Some(dt) = &payload.data_type {
            if dt == "url" {
                self.validate_url(&payload.content)?;
            } else if dt == "file" {
                self.validate_file_path(&payload.content)?;
            } else if dt == "image" {
                self.validate_image_payload(&payload.content)?;
            }
        }

        // 5. Validate any attached file paths
        for path in &payload.file_paths {
            self.validate_file_path(path)?;
        }

        Ok(())
    }

    /// Validates URL format.
    pub fn validate_url(&self, url_str: &str) -> Result<(), CaptureError> {
        let trimmed = url_str.trim();
        if trimmed.is_empty() {
            return Err(CaptureError::EmptyContent);
        }

        // Basic schema and host validation
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") && !trimmed.starts_with("file://") {
            return Err(CaptureError::InvalidUrl(format!("URL must start with http://, https://, or file:// (got: {})", trimmed)));
        }

        // Parse host
        let after_scheme = if let Some(stripped) = trimmed.strip_prefix("https://") {
            stripped
        } else if let Some(stripped) = trimmed.strip_prefix("http://") {
            stripped
        } else if let Some(stripped) = trimmed.strip_prefix("file://") {
            stripped
        } else {
            trimmed
        };

        if after_scheme.trim().is_empty() {
            return Err(CaptureError::InvalidUrl("URL host cannot be empty".to_string()));
        }

        Ok(())
    }

    /// Validates file path sanity.
    pub fn validate_file_path(&self, path_str: &str) -> Result<(), CaptureError> {
        let trimmed = path_str.trim();
        if trimmed.is_empty() {
            return Err(CaptureError::InvalidFile("File path cannot be empty".to_string()));
        }

        if trimmed.contains('\0') {
            return Err(CaptureError::InvalidFile("File path contains illegal null bytes".to_string()));
        }

        Ok(())
    }

    /// Validates image payload representation (data URI, base64, or file path).
    pub fn validate_image_payload(&self, image_str: &str) -> Result<(), CaptureError> {
        let trimmed = image_str.trim();
        if trimmed.is_empty() {
            return Err(CaptureError::InvalidImage("Image content is empty".to_string()));
        }

        if trimmed.starts_with("data:image/") || trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with('/') || trimmed.starts_with("C:\\") || trimmed.starts_with("file://") {
            return Ok(());
        }

        // Base64 or raw image
        if trimmed.len() > 20 {
            return Ok(());
        }

        Err(CaptureError::InvalidImage("Image format not recognized".to_string()))
    }
}

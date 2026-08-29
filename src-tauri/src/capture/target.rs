use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub data_type: String, // "text", "url", "image", "file"
    pub content: String,
    pub title: Option<String>,
    pub source: Option<String>,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub original_url: Option<String>,
    pub size: Option<usize>,
    pub workspace_id: Option<String>,
}

pub trait CaptureTarget: Send + Sync {
    fn show(&self) -> Result<(), String>;
    fn hide(&self) -> Result<(), String>;
    fn set_active(&self, active: bool) -> Result<(), String>;
    fn handle_drop(&self, request: CaptureRequest) -> Result<(), String>;
    fn report_success(&self, req: &CaptureRequest) -> Result<(), String>;
    fn report_failure(&self, error: &str) -> Result<(), String>;
}

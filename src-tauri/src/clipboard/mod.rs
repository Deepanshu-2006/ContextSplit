use clipboard_master::{Master, ClipboardHandler, CallbackResult};
use arboard::Clipboard;
use sha2::{Sha256, Digest};
use tauri::AppHandle;
use image::{RgbaImage, ImageFormat};
use base64::{engine::general_purpose, Engine as _};
use std::io::Cursor;
use std::time::Duration;

pub struct Monitor {
    app_handle: AppHandle,
    last_hash: String,
}

impl Monitor {
    pub fn new(app_handle: AppHandle) -> Self {
        let mut initial_hash = String::new();
        
        // Prevent duplicate capture on startup by pre-hashing the current clipboard
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                if !text.trim().is_empty() {
                    let mut hasher = Sha256::new();
                    hasher.update(text.as_bytes());
                    initial_hash = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
                }
            } else if let Ok(image) = clipboard.get_image() {
                let mut hasher = Sha256::new();
                hasher.update(&image.bytes);
                initial_hash = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
            }
        }
        
        Self {
            app_handle,
            last_hash: initial_hash,
        }
    }
    
    fn process_clipboard(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut clipboard = Clipboard::new()?;
        
        // Give the OS a tiny moment to actually write the data to the clipboard.
        // Sometimes the event fires before the data is fully available.
        std::thread::sleep(Duration::from_millis(50));
        
        // Try getting text first
        if let Ok(text) = clipboard.get_text() {
            if text.trim().is_empty() {
                return Ok(());
            }
            
            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            let hash = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();
            
            if hash == self.last_hash {
                return Ok(());
            }
            self.last_hash = hash;
            
            let is_url = text.starts_with("http://") || text.starts_with("https://");
            let item_type = if is_url { "url" } else { "text" };
            
            let app_handle = self.app_handle.clone();
            std::thread::spawn(move || {
                use crate::capture::{engine, target::CaptureRequest};
                
                let req = CaptureRequest {
                    data_type: item_type.to_string(),
                    content: text,
                    title: None,
                    source: Some("clipboard".to_string()),
                    filename: None,
                    mime_type: if is_url { Some("text/uri-list".to_string()) } else { Some("text/plain".to_string()) },
                    original_url: None,
                    size: None,
                    workspace_id: None,
                };
                
                let _ = engine::process_capture(app_handle, req);
            });
            return Ok(());
        }
        
        // Try getting image
        if let Ok(image) = clipboard.get_image() {
            let mut hasher = Sha256::new();
            hasher.update(&image.bytes);
            let hash = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();
            
            if hash == self.last_hash {
                return Ok(());
            }
            self.last_hash = hash;
            
            let app_handle = self.app_handle.clone();
            let image_bytes = image.bytes.into_owned();
            let width = image.width;
            let height = image.height;
            
            std::thread::spawn(move || {
                if let Some(rgba_image) = RgbaImage::from_raw(
                    width.try_into().unwrap_or(0),
                    height.try_into().unwrap_or(0),
                    image_bytes
                ) {
                    let mut buf = Cursor::new(Vec::new());
                    if rgba_image.write_to(&mut buf, ImageFormat::Png).is_ok() {
                        let b64 = general_purpose::STANDARD.encode(buf.into_inner());
                        let data_uri = format!("data:image/png;base64,{}", b64);
                        
                        use crate::capture::{engine, target::CaptureRequest};
                        let req = CaptureRequest {
                            data_type: "image".to_string(),
                            content: data_uri,
                            title: None,
                            source: Some("clipboard".to_string()),
                            filename: None,
                            mime_type: Some("image/png".to_string()),
                            original_url: None,
                            size: None,
                            workspace_id: None,
                        };
                        
                        let _ = engine::process_capture(app_handle, req);
                    }
                }
            });
            return Ok(());
        }
        
        Ok(())
    }
}

impl ClipboardHandler for Monitor {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        if let Err(e) = self.process_clipboard() {
            eprintln!("Clipboard processing error: {}", e);
        }
        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> CallbackResult {
        eprintln!("Clipboard monitor error: {}", error);
        CallbackResult::Next
    }
}

pub fn start_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        let monitor = Monitor::new(app_handle);
        let master_result = Master::new(monitor);
        if let Ok(mut master) = master_result {
            if let Err(e) = master.run() {
                eprintln!("Clipboard master run error: {}", e);
            }
        }
    });
}

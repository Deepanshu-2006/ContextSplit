use clipboard_master::{Master, ClipboardHandler, CallbackResult};
use arboard::Clipboard;
use sha2::{Sha256, Digest};
use tauri::{AppHandle, Emitter, Manager};
use crate::db::operations;
use crate::AppState;
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
        Self {
            app_handle,
            last_hash: String::new(),
        }
    }
    
    fn process_clipboard(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut clipboard = Clipboard::new()?;
        
        // Give the OS a tiny moment to actually write the data to the clipboard.
        // Sometimes the event fires before the data is fully available.
        std::thread::sleep(Duration::from_millis(50));
        
        let state = self.app_handle.state::<AppState>();
        
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
            
            if let Ok(conn) = state.db_conn.lock() {
                let item = operations::create_item(
                    &conn,
                    None, // Inbox
                    item_type,
                    &text,
                    None,
                    "{}"
                )?;
                let _ = self.app_handle.emit("clipboard://new-item", &item);
                
                #[cfg(target_os = "macos")]
                {
                    let (emoji, action) = if is_url { ("🔗", "Link grabbed") } else { ("📝", "Text saved") };
                    let _ = std::process::Command::new("osascript")
                        .arg("-e")
                        .arg(format!("display notification \"Ready in your Inbox\" with title \"ContextSplit\" subtitle \"{} {}\"", action, emoji))
                        .spawn();
                }
            }
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
            
            let rgba_image = RgbaImage::from_raw(
                image.width.try_into()?,
                image.height.try_into()?,
                image.bytes.into_owned()
            ).ok_or("Failed to create RgbaImage")?;
            
            let mut buf = Cursor::new(Vec::new());
            rgba_image.write_to(&mut buf, ImageFormat::Png)?;
            let b64 = general_purpose::STANDARD.encode(buf.into_inner());
            let data_uri = format!("data:image/png;base64,{}", b64);
            
            if let Ok(conn) = state.db_conn.lock() {
                let item = operations::create_item(
                    &conn,
                    None, // Inbox
                    "image",
                    &data_uri,
                    None,
                    "{}"
                )?;
                let _ = self.app_handle.emit("clipboard://new-item", &item);
                
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("osascript")
                        .arg("-e")
                        .arg("display notification \"Ready in your Inbox\" with title \"ContextSplit\" subtitle \"Image saved 🖼️\"")
                        .spawn();
                }
            }
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

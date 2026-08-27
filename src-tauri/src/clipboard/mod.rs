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
                let state = app_handle.state::<AppState>();
                if let Ok(conn) = state.db_conn.lock() {
                    if let Ok(item) = operations::create_item(
                        &conn,
                        None, // Inbox
                        item_type,
                        &text,
                        None,
                        "{}"
                    ) {
                        let _ = app_handle.emit("clipboard://new-item", &item);
                        
                        #[cfg(target_os = "macos")]
                        {
                            let (emoji, action) = if is_url { ("🔗", "Link grabbed") } else { ("📝", "Text saved") };
                            let _ = std::process::Command::new("osascript")
                                .arg("-e")
                                .arg(format!("display notification \"Ready in your Inbox\" with title \"ContextSplit\" subtitle \"{} {}\"", action, emoji))
                                .status(); // Use status() instead of spawn() to wait and reap the zombie process
                        }
                        
                        #[cfg(target_os = "windows")]
                        {
                            use tauri_plugin_notification::NotificationExt;
                            let (emoji, action) = if is_url { ("🔗", "Link grabbed") } else { ("📝", "Text saved") };
                            let _ = app_handle.notification()
                                .title("ContextSplit")
                                .body(format!("{} {}", action, emoji))
                                .show();
                        }
                    }
                };
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
                        
                        let state = app_handle.state::<AppState>();
                        if let Ok(conn) = state.db_conn.lock() {
                            if let Ok(item) = operations::create_item(
                                &conn,
                                None, // Inbox
                                "image",
                                &data_uri,
                                None,
                                "{}"
                            ) {
                                let _ = app_handle.emit("clipboard://new-item", &item);
                                
                                #[cfg(target_os = "macos")]
                                {
                                    let _ = std::process::Command::new("osascript")
                                        .arg("-e")
                                        .arg("display notification \"Ready in your Inbox\" with title \"ContextSplit\" subtitle \"Image saved 🖼️\"")
                                        .status();
                                }
                                
                                #[cfg(target_os = "windows")]
                                {
                                    use tauri_plugin_notification::NotificationExt;
                                    let _ = app_handle.notification()
                                        .title("ContextSplit")
                                        .body("Image saved 🖼️")
                                        .show();
                                }
                            }
                        };
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

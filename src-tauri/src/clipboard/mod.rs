use arboard::Clipboard;
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::capture::models::PlatformClipboardInput;
use crate::capture::{get_platform_adapter, CaptureCore};

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
                    initial_hash = hasher
                        .finalize()
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect();
                }
            } else if let Ok(image) = clipboard.get_image() {
                let mut hasher = Sha256::new();
                hasher.update(&image.bytes);
                initial_hash = hasher
                    .finalize()
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect();
            }
        }

        Self {
            app_handle,
            last_hash: initial_hash,
        }
    }

    fn process_clipboard(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(state) = self.app_handle.try_state::<crate::AppState>() {
            if state.capture_paused.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }
        }
        
        let mut clipboard = Clipboard::new()?;

        // Give the OS a tiny moment to write the data to the clipboard.
        std::thread::sleep(Duration::from_millis(50));

        let adapter = get_platform_adapter();

        // 1. Try getting text first
        if let Ok(text) = clipboard.get_text() {
            if text.trim().is_empty() {
                return Ok(());
            }

            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            let hash = hasher
                .finalize()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();

            if hash == self.last_hash {
                return Ok(());
            }
            self.last_hash = hash;

            let input = PlatformClipboardInput {
                text: Some(text),
                html: None,
                rtf: None,
                image_bytes: None,
                image_dimensions: None,
                file_urls: Vec::new(),
                active_app: None,
                window_title: None,
            };

            if let Ok(payload) = adapter.convert_clipboard_input(input) {
                let app_handle = self.app_handle.clone();
                std::thread::spawn(move || {
                    let core = CaptureCore::new();
                    let _ = core.process_app(app_handle, payload);
                });
            }
            return Ok(());
        }

        // 2. Try getting image
        if let Ok(image) = clipboard.get_image() {
            let mut hasher = Sha256::new();
            hasher.update(&image.bytes);
            let hash = hasher
                .finalize()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();

            if hash == self.last_hash {
                return Ok(());
            }
            self.last_hash = hash;

            let image_bytes = image.bytes.into_owned();
            let width: u32 = image.width.try_into().unwrap_or(0);
            let height: u32 = image.height.try_into().unwrap_or(0);

            let input = PlatformClipboardInput {
                text: None,
                html: None,
                rtf: None,
                image_bytes: Some(image_bytes),
                image_dimensions: Some((width, height)),
                file_urls: Vec::new(),
                active_app: None,
                window_title: None,
            };

            if let Ok(payload) = adapter.convert_clipboard_input(input) {
                let app_handle = self.app_handle.clone();
                std::thread::spawn(move || {
                    let core = CaptureCore::new();
                    let _ = core.process_app(app_handle, payload);
                });
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

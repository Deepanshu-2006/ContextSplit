use super::target::{CaptureTarget, CaptureRequest};
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WebviewUrl, PhysicalPosition, PhysicalSize};

pub struct MacOSTarget {
    app_handle: AppHandle,
}

impl MacOSTarget {
    pub fn new(app_handle: AppHandle) -> Self {
        // Initialize the overlay window
        if app_handle.get_webview_window("macos-notch-target").is_none() {
            let _ = WebviewWindowBuilder::new(
                &app_handle,
                "macos-notch-target",
                WebviewUrl::App("index.html".into())
            )
            .title("ContextSplit Capture")
            .inner_size(300.0, 100.0) // Adjust based on notch size
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .accept_first_mouse(true) // Allow drag and drop immediately
            .build()
            .map(|window| {
                // Position window at top center of primary monitor
                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let monitor_size = monitor.size();
                    let window_size = window.outer_size().unwrap_or(PhysicalSize::new(300, 100));
                    
                    let x = (monitor_size.width as i32 / 2) - (window_size.width as i32 / 2);
                    let y = 0; // Top of the screen
                    
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                }
                
                // TODO: Further macOS specific window styling (ignoresMouseEvents, NSWindowCollectionBehavior)
                // can be done via raw Objective-C hooks if needed, but for now Tauri's builder covers most.
            });
        }
        
        Self { app_handle }
    }
}

impl CaptureTarget for MacOSTarget {
    fn show(&self) -> Result<(), String> {
        if let Some(window) = self.app_handle.get_webview_window("macos-notch-target") {
            window.show().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn hide(&self) -> Result<(), String> {
        if let Some(window) = self.app_handle.get_webview_window("macos-notch-target") {
            window.hide().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn set_active(&self, active: bool) -> Result<(), String> {
        // TODO: Highlight the drop zone
        println!("MacOSTarget: set_active({})", active);
        Ok(())
    }

    fn handle_drop(&self, _request: CaptureRequest) -> Result<(), String> {
        // TODO: Pass the request to the existing capture engine
        println!("MacOSTarget: handle_drop()");
        Ok(())
    }

    fn report_success(&self, req: &CaptureRequest) -> Result<(), String> {
        let (emoji, action) = match req.data_type.as_str() {
            "url" => ("🔗", "Link grabbed"),
            "image" => ("🖼️", "Image saved"),
            "file" => ("📄", "File saved"),
            _ => ("📝", "Text saved"),
        };
        
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!("display notification \"Ready in your Inbox\" with title \"ContextSplit\" subtitle \"{} {}\"", action, emoji))
            .status(); 
            
        Ok(())
    }

    fn report_failure(&self, error: &str) -> Result<(), String> {
        // TODO: Trigger failure animation
        println!("MacOSTarget: report_failure({})", error);
        Ok(())
    }
}

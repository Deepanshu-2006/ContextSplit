use super::target::{CaptureTarget, CaptureRequest};
use super::notch::{NotchCaptureController, NotchState};
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WebviewUrl, PhysicalPosition, PhysicalSize, Emitter};
use std::sync::Arc;

pub struct MacOSTarget {
    app_handle: AppHandle,
    pub notch_controller: Arc<NotchCaptureController<Box<dyn Fn(NotchState) + Send + Sync>>>,
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
            });
        }
        
        let app_handle_clone = app_handle.clone();
        let notch_controller = Arc::new(NotchCaptureController::new(Box::new(move |state| {
            if let Some(window) = app_handle_clone.get_webview_window("macos-notch-target") {
                let _ = window.emit("target-state-changed", state);
                let _ = window.emit("notch-state-changed", state);
                
                match state {
                    NotchState::DragDetected | NotchState::Hover | NotchState::Active | NotchState::Captured | NotchState::Cancelled | NotchState::Paused => {
                        let _ = window.show();
                    }
                    NotchState::Idle => {
                        let _ = window.hide();
                    }
                }
            }
        }) as Box<dyn Fn(NotchState) + Send + Sync>));
        
        Self { app_handle, notch_controller }
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

    fn handle_drop(&self, request: CaptureRequest) -> Result<(), String> {
        super::engine::process_capture(self.app_handle.clone(), request)
    }

    fn report_success(&self, _req: &CaptureRequest) -> Result<(), String> {
        self.notch_controller.on_capture_success();
        Ok(())
    }

    fn report_failure(&self, error: &str) -> Result<(), String> {
        self.notch_controller.on_capture_failure();
        println!("MacOSTarget: report_failure({})", error);
        Ok(())
    }
}

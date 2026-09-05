use super::target::{CaptureTarget, CaptureRequest};
use super::windows_target::{WindowsCaptureTargetController, WindowsTargetState};
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WebviewUrl, PhysicalPosition, PhysicalSize, Emitter};
use std::sync::Arc;

pub struct WindowsTarget {
    app_handle: AppHandle,
    pub windows_controller: Arc<WindowsCaptureTargetController<Box<dyn Fn(WindowsTargetState) + Send + Sync>>>,
}

impl WindowsTarget {
    pub fn new(app_handle: AppHandle) -> Self {
        if app_handle.get_webview_window("windows-orb-target").is_none() {
            let _ = WebviewWindowBuilder::new(
                &app_handle,
                "windows-orb-target",
                WebviewUrl::App("index.html".into())
            )
            .title("ContextSplit Capture Orb")
            .inner_size(300.0, 150.0) // slightly taller for orb expansion
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(true)
            .accept_first_mouse(true)
            .build()
            .map(|window| {
                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let monitor_size = monitor.size();
                    let window_size = window.outer_size().unwrap_or(PhysicalSize::new(300, 150));
                    
                    // Top center, detached slightly
                    let x = (monitor_size.width as i32 / 2) - (window_size.width as i32 / 2);
                    let y = 32; 
                    
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                }
            });
        }
        
        let app_handle_clone = app_handle.clone();
        let windows_controller = Arc::new(WindowsCaptureTargetController::new(Box::new(move |state| {
            if let Some(window) = app_handle_clone.get_webview_window("windows-orb-target") {
                let _ = window.emit("target-state-changed", state);
                let _ = window.emit("windows-target-state-changed", state);
            }
        }) as Box<dyn Fn(WindowsTargetState) + Send + Sync>));
        
        Self { app_handle, windows_controller }
    }
}

impl CaptureTarget for WindowsTarget {
    fn show(&self) -> Result<(), String> {
        if let Some(window) = self.app_handle.get_webview_window("windows-orb-target") {
            window.show().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn hide(&self) -> Result<(), String> {
        if let Some(window) = self.app_handle.get_webview_window("windows-orb-target") {
            window.hide().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn set_active(&self, active: bool) -> Result<(), String> {
        println!("WindowsTarget: set_active({})", active);
        Ok(())
    }

    fn handle_drop(&self, request: CaptureRequest) -> Result<(), String> {
        super::engine::process_capture(self.app_handle.clone(), request)
    }

    fn report_success(&self, _req: &CaptureRequest) -> Result<(), String> {
        self.windows_controller.on_capture_success();
        Ok(())
    }

    fn report_failure(&self, error: &str) -> Result<(), String> {
        self.windows_controller.on_capture_failure();
        println!("WindowsTarget: report_failure({})", error);
        Ok(())
    }
}

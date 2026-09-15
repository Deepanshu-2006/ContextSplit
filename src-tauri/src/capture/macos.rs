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
        let app_handle_clone = app_handle.clone();
        let notch_controller = Arc::new(NotchCaptureController::new(Box::new(move |state| {
            if let Some(window) = app_handle_clone.get_webview_window("macos-notch-target") {
                let _ = window.emit("target-state-changed", state);
                let _ = window.emit("notch-state-changed", state);
            }
        }) as Box<dyn Fn(NotchState) + Send + Sync>));

        // Initialize the overlay window with native OS drag-and-drop handler
        if app_handle.get_webview_window("macos-notch-target").is_none() {
            let ctrl_drag = notch_controller.clone();
            let app_drag = app_handle.clone();

            let _ = WebviewWindowBuilder::new(
                &app_handle,
                "macos-notch-target",
                WebviewUrl::App("index.html".into())
            )
            .title("ContextSplit Capture Notch")
            .inner_size(480.0, 80.0) // Sized for notch + animated sliding wings
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .title_bar_style(tauri::TitleBarStyle::Transparent)
            .skip_taskbar(true)
            .visible(true)
            .shadow(false)
            .accept_first_mouse(true) // Allow drag and drop immediately
            .build()
            .map(|window| {
                #[cfg(target_os = "macos")]
                unsafe {
                    use objc::{msg_send, sel, sel_impl, class};
                    use cocoa::base::{id, nil, YES};
                    use cocoa::appkit::{NSWindow, NSWindowCollectionBehavior};
                    use cocoa::foundation::{NSRect, NSPoint, NSSize};

                    if let Ok(ns_win_ptr) = window.ns_window() {
                        let ns_win = ns_win_ptr as id;
                        if ns_win != nil {
                            // Ensure the window background is COMPLETELY transparent natively
                            ns_win.setOpaque_(cocoa::base::NO);
                            ns_win.setBackgroundColor_(cocoa::appkit::NSColor::clearColor(nil));

                            // NSStatusWindowLevel (25) floats directly above the menu bar and notch
                            ns_win.setLevel_(25);
                            ns_win.setCollectionBehavior_(
                                NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle
                            );

                            let nsscreen_class = class!(NSScreen);
                            let screen: id = msg_send![ns_win, screen];
                            let screen = if screen != nil { screen } else { msg_send![nsscreen_class, mainScreen] };
                            if screen != nil {
                                let sf: NSRect = msg_send![screen, frame];
                                let win_w = 480.0;
                                let win_h = 80.0;
                                let x = sf.origin.x + (sf.size.width - win_w) / 2.0;
                                let y = sf.origin.y + sf.size.height - win_h;
                                
                                let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(win_w, win_h));
                                ns_win.setFrame_display_(frame, YES);
                            }
                        }
                    }
                }

                let ctrl_drag = ctrl_drag.clone();
                let app_drag = app_drag.clone();
                window.on_window_event(move |event| {
                    match event {
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Enter { .. }) => {
                            ctrl_drag.on_drag_enter();
                        }
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Over { .. }) => {
                            ctrl_drag.on_drag_over(true);
                        }
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Leave) => {
                            ctrl_drag.on_drag_leave();
                        }
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                            if ctrl_drag.on_drop() {
                                // Instantly reset to Idle so the notch is immediately ready for the next rapid-fire drag
                                ctrl_drag.reset();
                                
                                let paths = paths.clone();
                                let app_drag = app_drag.clone();
                                std::thread::spawn(move || {
                                    for path in paths {
                                        let path_str = path.to_string_lossy().to_string();
                                        let filename = path.file_name().map(|f| f.to_string_lossy().to_string());
                                        let is_image = path.extension().map(|ext| {
                                            let e = ext.to_string_lossy().to_lowercase();
                                            e == "png" || e == "jpg" || e == "jpeg" || e == "webp" || e == "gif" || e == "svg"
                                        }).unwrap_or(false);

                                        let (content, mime_type) = if is_image {
                                            if let Ok(bytes) = std::fs::read(&path) {
                                                let mime = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
                                                    Some("jpg") | Some("jpeg") => "image/jpeg",
                                                    Some("png") => "image/png",
                                                    Some("gif") => "image/gif",
                                                    Some("webp") => "image/webp",
                                                    Some("svg") => "image/svg+xml",
                                                    _ => "image/png",
                                                };
                                                use base64::{engine::general_purpose, Engine as _};
                                                let b64 = general_purpose::STANDARD.encode(&bytes);
                                                (format!("data:{};base64,{}", mime, b64), Some(mime.to_string()))
                                            } else {
                                                (path_str.clone(), None)
                                            }
                                        } else {
                                            (path_str.clone(), None)
                                        };
                                        
                                        let req = CaptureRequest {
                                            data_type: if is_image { "image".to_string() } else { "file".to_string() },
                                            content,
                                            title: filename.clone(),
                                            source: Some("drag_drop".to_string()),
                                            filename,
                                            mime_type,
                                            original_url: None,
                                            size: std::fs::metadata(path).ok().map(|m| m.len() as usize),
                                            workspace_id: None,
                                        };
                                        let _ = super::engine::process_capture(app_drag.clone(), req);
                                    }
                                });
                            }
                        }
                        _ => {}
                    }
                });
            });
        }
        
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

use super::target::{CaptureTarget, CaptureRequest, TargetState};
use super::notch::{NotchInteractionController, NotchInteractionState, NotchGeometry, NotchCaptureEventPayload, NotchDebugSnapshot};
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WebviewUrl, Emitter};
use std::sync::Arc;

pub struct MacOSTarget {
    app_handle: AppHandle,
    pub notch_controller: Arc<NotchInteractionController<Box<dyn Fn(NotchInteractionState) + Send + Sync>>>,
}

impl MacOSTarget {
    pub fn new(app_handle: AppHandle) -> Self {
        let app_handle_clone = app_handle.clone();
        let notch_controller = Arc::new(NotchInteractionController::new(Box::new(move |state: NotchInteractionState| {
            if let Some(window) = app_handle_clone.get_webview_window("macos-notch-target") {
                // Emit both strongly typed notch states and legacy target states for UI compatibility
                let _ = window.emit("notch-state-changed", state);
                let legacy_state: TargetState = state.into();
                let _ = window.emit("target-state-changed", legacy_state);
            }
        }) as Box<dyn Fn(NotchInteractionState) + Send + Sync>));

        // Initialize the overlay window with native OS drag-and-drop handler
        if app_handle.get_webview_window("macos-notch-target").is_none() {
            let ctrl_drag = notch_controller.clone();
            let app_drag = app_handle.clone();

            let _ = WebviewWindowBuilder::new(
                &app_handle,
                "macos-notch-target",
                WebviewUrl::App("index.html".into())
            )
            .title("ContextSplit Notch Capture")
            .inner_size(600.0, 220.0) // Sized for physical notch + expanded island + intake chamber
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .title_bar_style(tauri::TitleBarStyle::Transparent)
            .skip_taskbar(true)
            .visible(true)
            .shadow(false)
            .accept_first_mouse(true) // Immediate drag and drop reception
            .build()
            .map(|window| {
                #[cfg(target_os = "macos")]
                unsafe {
                    use objc::{msg_send, sel, sel_impl, class};
                    use cocoa::base::{id, nil, YES};
                    use cocoa::appkit::{NSWindow, NSWindowCollectionBehavior};
                    use cocoa::foundation::{NSRect, NSPoint, NSSize};

                    #[repr(C)]
                    #[derive(Copy, Clone, Debug)]
                    struct NSEdgeInsets {
                        top: f64,
                        left: f64,
                        bottom: f64,
                        right: f64,
                    }

                    if let Ok(ns_win_ptr) = window.ns_window() {
                        let ns_win = ns_win_ptr as id;
                        if ns_win != nil {
                            // Ensure the window background is COMPLETELY transparent natively
                            ns_win.setOpaque_(cocoa::base::NO);
                            ns_win.setBackgroundColor_(cocoa::appkit::NSColor::clearColor(nil));
                            ns_win.setHasShadow_(cocoa::base::NO);
                            ns_win.setMovable_(cocoa::base::NO);

                            // NSStatusWindowLevel (25) floats directly above the menu bar and notch
                            ns_win.setLevel_(25);
                            ns_win.setCollectionBehavior_(
                                NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle
                            );

                            let nsscreen_class = class!(NSScreen);
                            let screens: id = msg_send![nsscreen_class, screens];
                            let count: usize = if screens != nil { msg_send![screens, count] } else { 0 };

                            let mut built_in_screen = nil;
                            for i in 0..count {
                                let s: id = msg_send![screens, objectAtIndex: i];
                                if s != nil {
                                    let insets: NSEdgeInsets = msg_send![s, safeAreaInsets];
                                    if insets.top > 0.0 {
                                        built_in_screen = s;
                                        break;
                                    }
                                }
                            }

                            let screen = if built_in_screen != nil {
                                built_in_screen
                            } else if count > 0 {
                                msg_send![screens, objectAtIndex: 0usize]
                            } else {
                                msg_send![nsscreen_class, mainScreen]
                            };

                            if screen != nil {
                                let sf: NSRect = msg_send![screen, frame];
                                let backing_scale: f64 = msg_send![screen, backingScaleFactor];
                                
                                let win_w = 600.0;
                                let win_h = 220.0;
                                let x = sf.origin.x + (sf.size.width - win_w) / 2.0;
                                let y = sf.origin.y + sf.size.height - win_h;
                                
                                let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(win_w, win_h));
                                ns_win.setFrame_display_(frame, YES);

                                // Update controller geometry with dynamic screen bounds & Retina scale
                                let dynamic_geom = NotchGeometry::for_display(
                                    "built_in_display",
                                    sf.size.width,
                                    sf.size.height,
                                    sf.origin.x,
                                    sf.origin.y,
                                    backing_scale,
                                    true,
                                    true,
                                );
                                ctrl_drag.update_geometry(dynamic_geom);
                            }
                        }
                    }
                }

                let ctrl_drag = ctrl_drag.clone();
                let app_drag = app_drag.clone();
                let win_clone = window.clone();
                window.on_window_event(move |event| {
                    match event {
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Enter { .. }) => {
                            #[cfg(target_os = "macos")]
                            unsafe {
                                use objc::{msg_send, sel, sel_impl, class};
                                use cocoa::base::{id, nil, YES};
                                use cocoa::foundation::{NSRect, NSPoint, NSSize};
                                use cocoa::appkit::NSWindow;

                                if let Ok(ns_win_ptr) = win_clone.ns_window() {
                                    let ns_win = ns_win_ptr as id;
                                    if ns_win != nil {
                                        let nsevent_class = class!(NSEvent);
                                        let mouse_loc: NSPoint = msg_send![nsevent_class, mouseLocation];

                                        let nsscreen_class = class!(NSScreen);
                                        let screens: id = msg_send![nsscreen_class, screens];
                                        let count: usize = if screens != nil { msg_send![screens, count] } else { 0 };

                                        let mut target_screen = nil;
                                        for i in 0..count {
                                            let s: id = msg_send![screens, objectAtIndex: i];
                                            if s != nil {
                                                let sf: NSRect = msg_send![s, frame];
                                                if mouse_loc.x >= sf.origin.x 
                                                    && mouse_loc.x <= sf.origin.x + sf.size.width
                                                    && mouse_loc.y >= sf.origin.y
                                                    && mouse_loc.y <= sf.origin.y + sf.size.height 
                                                {
                                                    target_screen = s;
                                                    break;
                                                }
                                            }
                                        }
                                        if target_screen != nil {
                                            let sf: NSRect = msg_send![target_screen, frame];
                                            let win_w = 600.0;
                                            let win_h = 220.0;
                                            let x = sf.origin.x + (sf.size.width - win_w) / 2.0;
                                            let y = sf.origin.y + sf.size.height - win_h;
                                            let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(win_w, win_h));
                                            ns_win.setFrame_display_(frame, YES);
                                        }
                                    }
                                }
                            }
                            ctrl_drag.on_drag_enter();
                        }
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Over { position }) => {
                            // Check if cursor is over the drop zone center
                            let is_center = position.x >= 130.0 && position.x <= 470.0 && position.y <= 135.0;
                            ctrl_drag.on_drag_over(is_center);
                        }
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Leave) => {
                            ctrl_drag.on_drag_leave();
                        }
                        tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, position: _ }) => {
                            if ctrl_drag.on_drop() {
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
                                        
                                        // Build canonical NotchCaptureEventPayload and forward to CaptureService
                                        let mut payload = NotchCaptureEventPayload::new(
                                            if is_image { "image" } else { "file" },
                                            content,
                                        );
                                        payload.mime_type = mime_type;
                                        payload.title = filename;

                                        let req = payload.into_capture_request();
                                        if let Err(e) = super::engine::process_capture(app_drag.clone(), req) {
                                            eprintln!("Capture error: {}", e);
                                        }
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

    pub fn get_debug_snapshot(&self, cursor_x: Option<f64>, cursor_y: Option<f64>) -> NotchDebugSnapshot {
        self.notch_controller.get_debug_snapshot(cursor_x, cursor_y)
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
        self.notch_controller.on_drag_over(active);
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

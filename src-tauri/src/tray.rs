use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub struct TrayMenuItems {
    pub status_item: MenuItem<tauri::Wry>,
    pub toggle_capture_item: MenuItem<tauri::Wry>,
}

static TRAY_ITEMS: Mutex<Option<TrayMenuItems>> = Mutex::new(None);

pub fn update_tray_capture_status(paused: bool) {
    if let Ok(guard) = TRAY_ITEMS.lock() {
        if let Some(items) = guard.as_ref() {
            let _ = items.status_item.set_text(if paused { "Capture Status: Paused" } else { "Capture Status: Active" });
            let _ = items.toggle_capture_item.set_text(if paused { "Resume Capture" } else { "Pause Capture" });
        }
    }
}

pub fn create_tray(app: &tauri::App) -> Result<(), tauri::Error> {
    let inbox_i = MenuItem::with_id(app, "inbox", "Inbox", true, None::<&str>)?;
    let status_i = MenuItem::with_id(app, "status", "Capture Status: Active", false, None::<&str>)?;
    let toggle_capture_i = MenuItem::with_id(app, "toggle_capture", "Pause Capture", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &inbox_i,
            &PredefinedMenuItem::separator(app)?,
            &status_i,
            &toggle_capture_i,
            &PredefinedMenuItem::separator(app)?,
            &settings_i,
            &PredefinedMenuItem::separator(app)?,
            &quit_i,
        ],
    )?;

    if let Ok(mut guard) = TRAY_ITEMS.lock() {
        *guard = Some(TrayMenuItems {
            status_item: status_i.clone(),
            toggle_capture_item: toggle_capture_i.clone(),
        });
    }

    let _tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app_handle, event| {
            match event.id.as_ref() {
                "inbox" => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "toggle_capture" => {
                    let state = app_handle.state::<crate::AppState>();
                    let current = state.capture_paused.load(std::sync::atomic::Ordering::Relaxed);
                    let _ = crate::set_capture_paused_internal(app_handle, !current);
                }
                "settings" => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.emit("navigate", "/settings");
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    app_handle.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .icon(app.default_window_icon().unwrap().clone())
        .build(app)?;

    Ok(())
}

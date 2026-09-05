use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager, Emitter
};

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

    let status_clone = status_i.clone();
    let toggle_capture_clone = toggle_capture_i.clone();

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
                    let new_state = !current;
                    state.capture_paused.store(new_state, std::sync::atomic::Ordering::Relaxed);
                    
                    let _ = status_clone.set_text(if new_state { "Capture Status: Paused" } else { "Capture Status: Active" });
                    let _ = toggle_capture_clone.set_text(if new_state { "Resume Capture" } else { "Pause Capture" });
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

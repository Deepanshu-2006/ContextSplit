// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Get the main window (our dropzone)
            if let Some(window) = app.get_webview_window("main") {
                // Position it near the notch (top center)
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let screen_size = monitor.size();
                    let window_size = window.outer_size().unwrap();
                    let x = (screen_size.width as i32 - window_size.width as i32) / 2;
                    let y = 10; // Slightly below the absolute top
                    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                }
                
                // Make it click-through so it doesn't block the user's mouse
                // Note: We will dynamically toggle this later when dragging starts
                let _ = window.set_ignore_cursor_events(true);
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

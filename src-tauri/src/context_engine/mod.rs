pub mod operations;
pub mod engine;
pub mod providers;
pub mod scoring;
pub mod relationships;
pub mod graph;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[derive(Debug, Clone)]
pub struct ActiveAppInfo {
    pub app_name: String,
    pub process_name: String,
    pub process_id: i32,
    pub window_title: Option<String>,
}

pub fn get_active_app() -> Option<ActiveAppInfo> {
    #[cfg(target_os = "macos")]
    {
        macos::active_app::get_active_app()
    }
    #[cfg(target_os = "windows")]
    {
        windows::active_app::get_active_app()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

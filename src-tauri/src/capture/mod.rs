pub mod target;
pub mod engine;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

pub use target::{CaptureTarget, CaptureRequest};

#[cfg(target_os = "macos")]
pub use macos::MacOSTarget as SystemTarget;

#[cfg(target_os = "windows")]
pub use windows::WindowsTarget as SystemTarget;

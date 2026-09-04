pub mod core;
pub mod engine;
pub mod models;
pub mod normalizer;
pub mod pipeline;
pub mod platform;
pub mod target;
pub mod validator;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
pub mod tests;

// Re-exports for public usage
pub use core::CaptureCore;
pub use pipeline::CapturePipeline;
pub use models::{
    CaptureError, CaptureEvent, CaptureMethod, CaptureResult, CaptureType, ContentType,
    ItemType, NormalizedCaptureEvent, PlatformClipboardInput, PlatformDragDropInput,
    PlatformManualInput, PlatformType, PlatformWindowContext, RawCapturePayload, SourceMetadata,
};
pub use normalizer::CaptureNormalizer;
pub use platform::{
    get_platform_adapter, MacOSCaptureAdapter, MockCaptureAdapter, PlatformCaptureAdapter,
    WindowsCaptureAdapter,
};
pub use target::{CaptureRequest, CaptureTarget};
pub use validator::CaptureValidator;

#[cfg(target_os = "macos")]
pub use macos::MacOSTarget as SystemTarget;

#[cfg(target_os = "windows")]
pub use windows::WindowsTarget as SystemTarget;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use target::CaptureTarget as SystemTarget;

pub mod macos;
pub mod mock;
pub mod windows;

pub use macos::MacOSCaptureAdapter;
pub use mock::MockCaptureAdapter;
pub use windows::WindowsCaptureAdapter;

use super::models::{
    CaptureError, PlatformClipboardInput, PlatformDragDropInput, PlatformManualInput,
    PlatformType, PlatformWindowContext, RawCapturePayload,
};

/// Trait implemented by platform-specific capture adapters.
///
/// CRITICAL ARCHITECTURAL RULE:
/// The platform layer is STRICTLY responsible for receiving OS/GUI inputs and
/// converting them into `RawCapturePayload`. It must NOT duplicate business logic,
/// database access, item persistence, or context engine processing.
pub trait PlatformCaptureAdapter: Send + Sync {
    /// Returns the platform type.
    fn platform_type(&self) -> PlatformType;

    /// Converts OS-level clipboard content into a raw capture payload.
    fn convert_clipboard_input(
        &self,
        input: PlatformClipboardInput,
    ) -> Result<RawCapturePayload, CaptureError>;

    /// Converts OS-level or UI drag & drop events into a raw capture payload.
    fn convert_drag_drop_input(
        &self,
        input: PlatformDragDropInput,
    ) -> Result<RawCapturePayload, CaptureError>;

    /// Converts user-initiated manual capture into a raw capture payload.
    fn convert_manual_input(
        &self,
        input: PlatformManualInput,
    ) -> Result<RawCapturePayload, CaptureError>;

    /// Retrieves current active window / process context if available on this platform.
    fn get_active_window_context(&self) -> Option<PlatformWindowContext>;
}

/// Returns the default platform capture adapter for the current host environment.
pub fn get_platform_adapter() -> Box<dyn PlatformCaptureAdapter> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacOSCaptureAdapter::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsCaptureAdapter::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Box::new(MockCaptureAdapter::new(PlatformType::Generic))
    }
}

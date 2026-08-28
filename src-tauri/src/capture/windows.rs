use super::target::{CaptureTarget, CaptureRequest};

pub struct WindowsTarget {
    // Windows specific handles (e.g. HWND or similar) can go here
}

impl WindowsTarget {
    pub fn new() -> Self {
        Self {}
    }
}

impl CaptureTarget for WindowsTarget {
    fn show(&self) -> Result<(), String> {
        // TODO: Implement Windows specific show logic (e.g. Capture Orb)
        println!("WindowsTarget: show()");
        Ok(())
    }

    fn hide(&self) -> Result<(), String> {
        // TODO: Implement Windows specific hide logic
        println!("WindowsTarget: hide()");
        Ok(())
    }

    fn set_active(&self, active: bool) -> Result<(), String> {
        // TODO: Highlight the drop zone
        println!("WindowsTarget: set_active({})", active);
        Ok(())
    }

    fn handle_drop(&self, _request: CaptureRequest) -> Result<(), String> {
        // TODO: Pass the request to the existing capture engine
        println!("WindowsTarget: handle_drop()");
        Ok(())
    }

    fn report_success(&self) -> Result<(), String> {
        // TODO: Trigger success animation
        println!("WindowsTarget: report_success()");
        Ok(())
    }

    fn report_failure(&self, error: &str) -> Result<(), String> {
        // TODO: Trigger failure animation
        println!("WindowsTarget: report_failure({})", error);
        Ok(())
    }
}

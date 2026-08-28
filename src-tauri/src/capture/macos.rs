use super::target::{CaptureTarget, CaptureRequest};

pub struct MacOSTarget {
    // macOS specific handles (e.g. NSWindow or similar) can go here
}

impl MacOSTarget {
    pub fn new() -> Self {
        Self {}
    }
}

impl CaptureTarget for MacOSTarget {
    fn show(&self) -> Result<(), String> {
        // TODO: Implement macOS specific show logic (e.g. Notch drop zone)
        println!("MacOSTarget: show()");
        Ok(())
    }

    fn hide(&self) -> Result<(), String> {
        // TODO: Implement macOS specific hide logic
        println!("MacOSTarget: hide()");
        Ok(())
    }

    fn set_active(&self, active: bool) -> Result<(), String> {
        // TODO: Highlight the drop zone
        println!("MacOSTarget: set_active({})", active);
        Ok(())
    }

    fn handle_drop(&self, _request: CaptureRequest) -> Result<(), String> {
        // TODO: Pass the request to the existing capture engine
        println!("MacOSTarget: handle_drop()");
        Ok(())
    }

    fn report_success(&self) -> Result<(), String> {
        // TODO: Trigger success animation
        println!("MacOSTarget: report_success()");
        Ok(())
    }

    fn report_failure(&self, error: &str) -> Result<(), String> {
        // TODO: Trigger failure animation
        println!("MacOSTarget: report_failure({})", error);
        Ok(())
    }
}

use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowsTargetState {
    Idle,
    Prominent,
    Activated,
    Captured,
    Cancelled,
}

pub struct WindowsCaptureTargetController<F>
where
    F: Fn(WindowsTargetState) + Send + Sync + 'static,
{
    state: Arc<Mutex<WindowsTargetState>>,
    on_state_change: F,
}

impl<F> WindowsCaptureTargetController<F>
where
    F: Fn(WindowsTargetState) + Send + Sync + 'static,
{
    pub fn new(on_state_change: F) -> Self {
        // Initial state is always Idle
        on_state_change(WindowsTargetState::Idle);
        
        Self {
            state: Arc::new(Mutex::new(WindowsTargetState::Idle)),
            on_state_change,
        }
    }

    pub fn get_state(&self) -> WindowsTargetState {
        *self.state.lock().unwrap()
    }

    fn transition_to(&self, new_state: WindowsTargetState) {
        let mut current = self.state.lock().unwrap();
        *current = new_state;
        (self.on_state_change)(new_state);
    }

    pub fn on_drag_enter(&self) {
        let state = self.get_state();
        if state == WindowsTargetState::Idle || state == WindowsTargetState::Cancelled {
            self.transition_to(WindowsTargetState::Prominent);
        }
    }

    pub fn on_drag_over(&self, is_inside_target: bool) {
        let state = self.get_state();
        match state {
            WindowsTargetState::Prominent if is_inside_target => {
                self.transition_to(WindowsTargetState::Activated);
            }
            WindowsTargetState::Activated if !is_inside_target => {
                self.transition_to(WindowsTargetState::Prominent);
            }
            _ => {}
        }
    }

    pub fn on_drag_leave(&self) {
        let state = self.get_state();
        if state == WindowsTargetState::Prominent || state == WindowsTargetState::Activated {
            self.transition_to(WindowsTargetState::Cancelled);
            // After cancelled, return to idle
            self.transition_to(WindowsTargetState::Idle);
        }
    }

    pub fn on_drop(&self) -> bool {
        let state = self.get_state();
        if state == WindowsTargetState::Activated {
            // Drop accepted
            self.transition_to(WindowsTargetState::Idle);
            true
        } else {
            // Drop rejected because not activated
            self.transition_to(WindowsTargetState::Cancelled);
            self.transition_to(WindowsTargetState::Idle);
            false
        }
    }

    pub fn on_capture_success(&self) {
        self.transition_to(WindowsTargetState::Captured);
        self.transition_to(WindowsTargetState::Idle);
    }

    pub fn on_capture_failure(&self) {
        self.transition_to(WindowsTargetState::Cancelled);
        self.transition_to(WindowsTargetState::Idle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_idle_to_prominent() {
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_clone = changes.clone();
        
        let controller = WindowsCaptureTargetController::new(move |state| {
            changes_clone.lock().unwrap().push(state);
        });

        assert_eq!(controller.get_state(), WindowsTargetState::Idle);
        
        controller.on_drag_enter();
        assert_eq!(controller.get_state(), WindowsTargetState::Prominent);
        
        let hist = changes.lock().unwrap();
        assert_eq!(*hist, vec![WindowsTargetState::Idle, WindowsTargetState::Prominent]);
    }

    #[test]
    fn test_prominent_to_activated_and_back() {
        let controller = WindowsCaptureTargetController::new(|_| {});
        
        controller.on_drag_enter();
        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), WindowsTargetState::Activated);
        
        controller.on_drag_over(false);
        assert_eq!(controller.get_state(), WindowsTargetState::Prominent);
    }

    #[test]
    fn test_drag_leave_cancels_to_idle() {
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_clone = changes.clone();
        let controller = WindowsCaptureTargetController::new(move |state| {
            changes_clone.lock().unwrap().push(state);
        });

        controller.on_drag_enter();
        controller.on_drag_leave();
        assert_eq!(controller.get_state(), WindowsTargetState::Idle);

        let hist = changes.lock().unwrap();
        assert_eq!(*hist, vec![
            WindowsTargetState::Idle, 
            WindowsTargetState::Prominent, 
            WindowsTargetState::Cancelled, 
            WindowsTargetState::Idle
        ]);
    }

    #[test]
    fn test_drop_when_activated() {
        let controller = WindowsCaptureTargetController::new(|_| {});
        
        controller.on_drag_enter();
        controller.on_drag_over(true);
        
        let accepted = controller.on_drop();
        assert!(accepted);
        assert_eq!(controller.get_state(), WindowsTargetState::Idle);
    }

    #[test]
    fn test_drop_when_prominent_rejected() {
        let controller = WindowsCaptureTargetController::new(|_| {});
        
        controller.on_drag_enter(); // Prominent, but not activated
        
        let accepted = controller.on_drop();
        assert!(!accepted);
        assert_eq!(controller.get_state(), WindowsTargetState::Idle);
    }

    #[test]
    fn test_capture_success_and_failure() {
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_clone = changes.clone();
        let controller = WindowsCaptureTargetController::new(move |state| {
            changes_clone.lock().unwrap().push(state);
        });

        controller.on_capture_success();
        controller.on_capture_failure();

        let hist = changes.lock().unwrap();
        assert_eq!(*hist, vec![
            WindowsTargetState::Idle,
            WindowsTargetState::Captured,
            WindowsTargetState::Idle,
            WindowsTargetState::Cancelled,
            WindowsTargetState::Idle
        ]);
    }
}

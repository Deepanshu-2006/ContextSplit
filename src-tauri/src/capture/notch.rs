use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NotchState {
    Idle,
    Hovering,
    Activated,
    Captured,
    Cancelled,
}

pub struct NotchCaptureController<F>
where
    F: Fn(NotchState) + Send + Sync + 'static,
{
    state: Arc<Mutex<NotchState>>,
    on_state_change: F,
}

impl<F> NotchCaptureController<F>
where
    F: Fn(NotchState) + Send + Sync + 'static,
{
    pub fn new(on_state_change: F) -> Self {
        // Initial state is always Idle
        on_state_change(NotchState::Idle);
        
        Self {
            state: Arc::new(Mutex::new(NotchState::Idle)),
            on_state_change,
        }
    }

    pub fn get_state(&self) -> NotchState {
        *self.state.lock().unwrap()
    }

    fn transition_to(&self, new_state: NotchState) {
        let mut current = self.state.lock().unwrap();
        *current = new_state;
        (self.on_state_change)(new_state);
    }

    pub fn on_drag_enter(&self) {
        let state = self.get_state();
        if state == NotchState::Idle || state == NotchState::Cancelled {
            self.transition_to(NotchState::Hovering);
        }
    }

    pub fn on_drag_over(&self, is_near_center: bool) {
        let state = self.get_state();
        match state {
            NotchState::Hovering if is_near_center => {
                self.transition_to(NotchState::Activated);
            }
            NotchState::Activated if !is_near_center => {
                self.transition_to(NotchState::Hovering);
            }
            _ => {}
        }
    }

    pub fn on_drag_leave(&self) {
        let state = self.get_state();
        if state == NotchState::Hovering || state == NotchState::Activated {
            self.transition_to(NotchState::Cancelled);
            // After cancelled, we return to idle
            self.transition_to(NotchState::Idle);
        }
    }

    pub fn on_drop(&self) -> bool {
        let state = self.get_state();
        if state == NotchState::Activated {
            // Drop accepted
            self.transition_to(NotchState::Idle); // Wait for success/failure to show Captured
            true
        } else {
            // Drop rejected because not activated
            self.transition_to(NotchState::Cancelled);
            self.transition_to(NotchState::Idle);
            false
        }
    }

    pub fn on_capture_success(&self) {
        self.transition_to(NotchState::Captured);
        self.transition_to(NotchState::Idle);
    }

    pub fn on_capture_failure(&self) {
        self.transition_to(NotchState::Cancelled);
        self.transition_to(NotchState::Idle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_idle_to_hovering() {
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_clone = changes.clone();
        
        let controller = NotchCaptureController::new(move |state| {
            changes_clone.lock().unwrap().push(state);
        });

        assert_eq!(controller.get_state(), NotchState::Idle);
        
        controller.on_drag_enter();
        assert_eq!(controller.get_state(), NotchState::Hovering);
        
        let hist = changes.lock().unwrap();
        assert_eq!(*hist, vec![NotchState::Idle, NotchState::Hovering]);
    }

    #[test]
    fn test_hovering_to_activated_and_back() {
        let controller = NotchCaptureController::new(|_| {});
        
        controller.on_drag_enter();
        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), NotchState::Activated);
        
        controller.on_drag_over(false);
        assert_eq!(controller.get_state(), NotchState::Hovering);
    }

    #[test]
    fn test_drag_leave_cancels() {
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_clone = changes.clone();
        let controller = NotchCaptureController::new(move |state| {
            changes_clone.lock().unwrap().push(state);
        });

        controller.on_drag_enter();
        controller.on_drag_leave();
        assert_eq!(controller.get_state(), NotchState::Idle);

        let hist = changes.lock().unwrap();
        // Initial idle, hovering, cancelled, then back to idle
        assert_eq!(*hist, vec![
            NotchState::Idle, 
            NotchState::Hovering, 
            NotchState::Cancelled, 
            NotchState::Idle
        ]);
    }

    #[test]
    fn test_drop_when_activated() {
        let controller = NotchCaptureController::new(|_| {});
        
        controller.on_drag_enter();
        controller.on_drag_over(true);
        
        let accepted = controller.on_drop();
        assert!(accepted);
        assert_eq!(controller.get_state(), NotchState::Idle);
    }

    #[test]
    fn test_drop_when_hovering_rejected() {
        let controller = NotchCaptureController::new(|_| {});
        
        controller.on_drag_enter(); // Hovering, but not activated
        
        let accepted = controller.on_drop();
        assert!(!accepted);
        assert_eq!(controller.get_state(), NotchState::Idle);
    }
}

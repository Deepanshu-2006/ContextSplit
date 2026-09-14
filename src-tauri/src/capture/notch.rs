use super::target::{CaptureTargetController, TargetState};

/// Backwards-compatibility type alias for macOS Notch.
pub type NotchState = TargetState;

/// Backwards-compatibility type alias for macOS Notch Controller.
pub type NotchCaptureController<F> = CaptureTargetController<F>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_notch_state_transitions() {
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_clone = changes.clone();

        let controller = NotchCaptureController::new(move |state| {
            changes_clone.lock().unwrap().push(state);
        });

        assert_eq!(controller.get_state(), TargetState::Idle);

        controller.on_drag_enter();
        assert_eq!(controller.get_state(), TargetState::Hover);

        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), TargetState::Active);

        let accepted = controller.on_drop();
        assert!(accepted);
        assert_eq!(controller.get_state(), TargetState::Captured);

        let hist = changes.lock().unwrap();
        assert_eq!(
            *hist,
            vec![
                TargetState::Idle,
                TargetState::Hover,
                TargetState::Active,
                TargetState::Captured
            ]
        );
    }
}

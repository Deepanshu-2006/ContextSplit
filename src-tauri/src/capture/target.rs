use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

/// Shared, platform-independent states for CaptureTarget overlays across macOS, Windows, and Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetState {
    /// Target is idle/unobtrusive (or hidden).
    Idle,
    /// Drag action carrying supported content has been detected globally.
    DragDetected,
    /// Dragged content is hovering near/over the target area.
    Hover,
    /// Dragged content is directly over the drop/activation target ("Release to capture").
    Active,
    /// Content was dropped and accepted; displaying capture confirmation.
    Captured,
    /// Drag left the target area or drop was cancelled/rejected; returning to Idle.
    Cancelled,
    /// Automatic capture is paused; indicates paused status and rejects drops.
    Paused,
}

impl std::fmt::Display for TargetState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::DragDetected => write!(f, "drag_detected"),
            Self::Hover => write!(f, "hover"),
            Self::Active => write!(f, "active"),
            Self::Captured => write!(f, "captured"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Paused => write!(f, "paused"),
        }
    }
}

impl From<&str> for TargetState {
    fn from(s: &str) -> Self {
        match s.to_lowercase().replace('-', "_").as_str() {
            "drag_detected" | "dragdetected" => Self::DragDetected,
            "hover" | "hovering" => Self::Hover,
            "active" | "activated" => Self::Active,
            "captured" | "success" => Self::Captured,
            "cancelled" | "canceled" | "failure" => Self::Cancelled,
            "paused" | "pause" => Self::Paused,
            _ => Self::Idle,
        }
    }
}

/// Platform-independent controller managing TargetState transitions.
pub struct CaptureTargetController<F>
where
    F: Fn(TargetState) + Send + Sync + 'static,
{
    state: Arc<Mutex<TargetState>>,
    is_paused: Arc<AtomicBool>,
    on_state_change: F,
}

impl<F> CaptureTargetController<F>
where
    F: Fn(TargetState) + Send + Sync + 'static,
{
    pub fn new(on_state_change: F) -> Self {
        // Initial state is always Idle
        on_state_change(TargetState::Idle);
        Self {
            state: Arc::new(Mutex::new(TargetState::Idle)),
            is_paused: Arc::new(AtomicBool::new(false)),
            on_state_change,
        }
    }

    pub fn get_state(&self) -> TargetState {
        *self.state.lock().unwrap()
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.is_paused.store(paused, Ordering::Relaxed);
        if paused {
            let state = self.get_state();
            if state != TargetState::Idle && state != TargetState::Cancelled && state != TargetState::Captured {
                self.transition_to(TargetState::Paused);
            }
        } else {
            let state = self.get_state();
            if state == TargetState::Paused {
                self.transition_to(TargetState::Idle);
            }
        }
    }

    fn transition_to(&self, new_state: TargetState) {
        let mut current = self.state.lock().unwrap();
        *current = new_state;
        (self.on_state_change)(new_state);
    }

    /// User begins dragging supported content across the system.
    pub fn on_drag_detected(&self) {
        if self.is_paused() {
            self.transition_to(TargetState::Paused);
            return;
        }
        let state = self.get_state();
        if state == TargetState::Idle || state == TargetState::Cancelled {
            self.transition_to(TargetState::DragDetected);
        }
    }

    /// Dragged content enters the target bounds or proximity window.
    pub fn on_drag_enter(&self) {
        if self.is_paused() {
            self.transition_to(TargetState::Paused);
            return;
        }
        let state = self.get_state();
        if state == TargetState::Idle || state == TargetState::DragDetected || state == TargetState::Cancelled {
            self.transition_to(TargetState::Hover);
        }
    }

    /// Drag position updates relative to target center/activation zone.
    pub fn on_drag_over(&self, is_active_zone: bool) {
        if self.is_paused() {
            self.transition_to(TargetState::Paused);
            return;
        }
        let state = self.get_state();
        match state {
            TargetState::Hover | TargetState::DragDetected if is_active_zone => {
                self.transition_to(TargetState::Active);
            }
            TargetState::Active if !is_active_zone => {
                self.transition_to(TargetState::Hover);
            }
            _ => {}
        }
    }

    /// Drag left the target area.
    pub fn on_drag_leave(&self) {
        let state = self.get_state();
        if state == TargetState::Paused {
            self.transition_to(TargetState::Idle);
            return;
        }
        if state == TargetState::Hover || state == TargetState::Active || state == TargetState::DragDetected {
            self.transition_to(TargetState::Cancelled);
            self.transition_to(TargetState::Idle);
        }
    }

    /// Content was dropped. Returns `true` if accepted (when not paused), `false` if rejected.
    pub fn on_drop(&self) -> bool {
        if self.is_paused() {
            self.transition_to(TargetState::Cancelled);
            self.transition_to(TargetState::Idle);
            return false;
        }
        self.transition_to(TargetState::Captured);
        true
    }

    /// Core processing completed successfully -> displays Captured confirmation then returns to Idle.
    pub fn on_capture_success(&self) {
        if self.get_state() != TargetState::Captured {
            self.transition_to(TargetState::Captured);
        }
        self.transition_to(TargetState::Idle);
    }

    /// Core processing failed -> displays Cancelled indicator then returns to Idle.
    pub fn on_capture_failure(&self) {
        if self.get_state() != TargetState::Cancelled {
            self.transition_to(TargetState::Cancelled);
        }
        self.transition_to(TargetState::Idle);
    }

    /// Resets target directly to Idle.
    pub fn reset(&self) {
        self.transition_to(TargetState::Idle);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub data_type: String, // "text", "url", "image", "file"
    pub content: String,
    pub title: Option<String>,
    pub source: Option<String>,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub original_url: Option<String>,
    pub size: Option<usize>,
    pub workspace_id: Option<String>,
}

pub trait CaptureTarget: Send + Sync {
    fn show(&self) -> Result<(), String>;
    fn hide(&self) -> Result<(), String>;
    fn set_active(&self, active: bool) -> Result<(), String>;
    fn handle_drop(&self, request: CaptureRequest) -> Result<(), String>;
    fn report_success(&self, req: &CaptureRequest) -> Result<(), String>;
    fn report_failure(&self, error: &str) -> Result<(), String>;
}

// =============================================================================
// State Transition Test Suite
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_initial_state_is_idle() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        assert_eq!(controller.get_state(), TargetState::Idle);
        assert_eq!(*history.lock().unwrap(), vec![TargetState::Idle]);
    }

    #[test]
    fn test_idle_to_drag_detected() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_drag_detected();
        assert_eq!(controller.get_state(), TargetState::DragDetected);
        assert_eq!(*history.lock().unwrap(), vec![TargetState::Idle, TargetState::DragDetected]);
    }

    #[test]
    fn test_drag_detected_to_hover() {
        let controller = CaptureTargetController::new(|_| {});
        controller.on_drag_detected();
        assert_eq!(controller.get_state(), TargetState::DragDetected);

        controller.on_drag_enter();
        assert_eq!(controller.get_state(), TargetState::Hover);
    }

    #[test]
    fn test_hover_to_active_and_back() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_drag_enter();
        assert_eq!(controller.get_state(), TargetState::Hover);

        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), TargetState::Active);

        controller.on_drag_over(false);
        assert_eq!(controller.get_state(), TargetState::Hover);

        assert_eq!(
            *history.lock().unwrap(),
            vec![
                TargetState::Idle,
                TargetState::Hover,
                TargetState::Active,
                TargetState::Hover
            ]
        );
    }

    #[test]
    fn test_direct_idle_to_hover() {
        let controller = CaptureTargetController::new(|_| {});
        controller.on_drag_enter();
        assert_eq!(controller.get_state(), TargetState::Hover);
    }

    #[test]
    fn test_direct_drag_detected_to_active() {
        let controller = CaptureTargetController::new(|_| {});
        controller.on_drag_detected();
        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), TargetState::Active);
    }

    #[test]
    fn test_drag_leave_from_hover_transitions_cancelled_to_idle() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_drag_enter();
        controller.on_drag_leave();

        assert_eq!(controller.get_state(), TargetState::Idle);
        assert_eq!(
            *history.lock().unwrap(),
            vec![
                TargetState::Idle,
                TargetState::Hover,
                TargetState::Cancelled,
                TargetState::Idle
            ]
        );
    }

    #[test]
    fn test_drag_leave_from_active_transitions_cancelled_to_idle() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_drag_enter();
        controller.on_drag_over(true);
        controller.on_drag_leave();

        assert_eq!(controller.get_state(), TargetState::Idle);
        assert_eq!(
            *history.lock().unwrap(),
            vec![
                TargetState::Idle,
                TargetState::Hover,
                TargetState::Active,
                TargetState::Cancelled,
                TargetState::Idle
            ]
        );
    }

    #[test]
    fn test_drag_leave_from_drag_detected() {
        let controller = CaptureTargetController::new(|_| {});
        controller.on_drag_detected();
        controller.on_drag_leave();
        assert_eq!(controller.get_state(), TargetState::Idle);
    }

    #[test]
    fn test_drop_when_active_accepted() {
        let controller = CaptureTargetController::new(|_| {});
        controller.on_drag_enter();
        controller.on_drag_over(true);

        let accepted = controller.on_drop();
        assert!(accepted);
        assert_eq!(controller.get_state(), TargetState::Captured);
    }

    #[test]
    fn test_drop_when_hover_accepted() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_drag_enter();
        let accepted = controller.on_drop();

        assert!(accepted);
        assert_eq!(controller.get_state(), TargetState::Captured);
    }

    #[test]
    fn test_capture_success_lifecycle() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_capture_success();
        assert_eq!(controller.get_state(), TargetState::Idle);
        assert_eq!(
            *history.lock().unwrap(),
            vec![
                TargetState::Idle,
                TargetState::Captured,
                TargetState::Idle
            ]
        );
    }

    #[test]
    fn test_capture_failure_lifecycle() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_capture_failure();
        assert_eq!(controller.get_state(), TargetState::Idle);
        assert_eq!(
            *history.lock().unwrap(),
            vec![
                TargetState::Idle,
                TargetState::Cancelled,
                TargetState::Idle
            ]
        );
    }

    #[test]
    fn test_reset_to_idle() {
        let controller = CaptureTargetController::new(|_| {});
        controller.on_drag_enter();
        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), TargetState::Active);

        controller.reset();
        assert_eq!(controller.get_state(), TargetState::Idle);
    }

    #[test]
    fn test_full_platform_independent_lifecycle() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        // 1. Idle -> DragDetected (global drag begins)
        controller.on_drag_detected();
        // 2. DragDetected -> Hover (enters target area)
        controller.on_drag_enter();
        // 3. Hover -> Active (moves to center release zone)
        controller.on_drag_over(true);
        // 4. Release in active zone -> Accepted
        let accepted = controller.on_drop();
        assert!(accepted);
        // 5. Capture Core persists item -> on_capture_success
        controller.on_capture_success();

        assert_eq!(controller.get_state(), TargetState::Idle);
        assert_eq!(
            *history.lock().unwrap(),
            vec![
                TargetState::Idle,
                TargetState::DragDetected,
                TargetState::Hover,
                TargetState::Active,
                TargetState::Captured,
                TargetState::Idle
            ]
        );
    }

    #[test]
    fn test_target_controller_paused_state_transitions() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = CaptureTargetController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        // Set paused
        controller.set_paused(true);
        assert!(controller.is_paused());

        // Drag detected while paused -> transitions to Paused
        controller.on_drag_detected();
        assert_eq!(controller.get_state(), TargetState::Paused);

        // Drag enters target area while paused -> remains in Paused
        controller.on_drag_enter();
        assert_eq!(controller.get_state(), TargetState::Paused);

        // Drag over active zone while paused -> remains in Paused
        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), TargetState::Paused);

        // Drop while paused -> rejected (returns false) and resets to Idle
        let accepted = controller.on_drop();
        assert!(!accepted);
        assert_eq!(controller.get_state(), TargetState::Idle);

        // Resume capture
        controller.set_paused(false);
        assert!(!controller.is_paused());

        // Drag enter now transitions to Hover normally
        controller.on_drag_enter();
        assert_eq!(controller.get_state(), TargetState::Hover);
    }

    #[test]
    fn test_target_controller_pause_while_hovering() {
        let controller = CaptureTargetController::new(|_| {});
        controller.on_drag_enter();
        assert_eq!(controller.get_state(), TargetState::Hover);

        // Pausing while active/hover transitions immediately to Paused
        controller.set_paused(true);
        assert_eq!(controller.get_state(), TargetState::Paused);

        // Drag leave resets to Idle
        controller.on_drag_leave();
        assert_eq!(controller.get_state(), TargetState::Idle);
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use super::models::{CaptureEvent, CaptureType, ContentType, PlatformType};
use super::target::{CaptureRequest, TargetState};

/// Canonical Interaction States for the macOS Notch Capture surface.
///
/// States follow a physical spring lifecycle:
/// IDLE -> DRAG_DETECTED -> APPROACHING -> DROP_TARGET -> CAPTURING -> SUCCESS -> RETURN_TO_IDLE -> IDLE
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotchInteractionState {
    /// Notch is resting in its natural hardware state with minimal/no visual presence.
    Idle,
    /// A system drag has been detected globally (or entered the window bounds).
    DragDetected,
    /// Cursor with dragged content is approaching the notch within the activation radius.
    Approaching,
    /// Cursor is directly inside the valid capture drop target zone.
    DropTarget,
    /// Item was dropped; capture payload is being ingested and compressed.
    Capturing,
    /// Capture succeeded; displaying confirmation checkmark feedback.
    Success,
    /// Notch surface is smoothly collapsing via spring physics back to its resting state.
    ReturnToIdle,
    /// Automatic notch capture is paused by user or system policy.
    Paused,
}

impl std::fmt::Display for NotchInteractionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::DragDetected => write!(f, "drag_detected"),
            Self::Approaching => write!(f, "approaching"),
            Self::DropTarget => write!(f, "drop_target"),
            Self::Capturing => write!(f, "capturing"),
            Self::Success => write!(f, "success"),
            Self::ReturnToIdle => write!(f, "return_to_idle"),
            Self::Paused => write!(f, "paused"),
        }
    }
}

impl From<&str> for NotchInteractionState {
    fn from(s: &str) -> Self {
        match s.to_lowercase().replace('-', "_").as_str() {
            "idle" => Self::Idle,
            "drag_detected" | "dragdetected" => Self::DragDetected,
            "approaching" | "hover" | "hovering" => Self::Approaching,
            "drop_target" | "droptarget" | "active" | "activated" => Self::DropTarget,
            "capturing" => Self::Capturing,
            "success" | "captured" => Self::Success,
            "return_to_idle" | "returntoidle" | "cancelled" | "canceled" => Self::ReturnToIdle,
            "paused" | "pause" => Self::Paused,
            _ => Self::Idle,
        }
    }
}

impl From<TargetState> for NotchInteractionState {
    fn from(state: TargetState) -> Self {
        match state {
            TargetState::Idle => Self::Idle,
            TargetState::DragDetected => Self::DragDetected,
            TargetState::Hover => Self::Approaching,
            TargetState::Active => Self::DropTarget,
            TargetState::Captured => Self::Success,
            TargetState::Cancelled => Self::ReturnToIdle,
            TargetState::Paused => Self::Paused,
        }
    }
}

impl From<NotchInteractionState> for TargetState {
    fn from(state: NotchInteractionState) -> Self {
        match state {
            NotchInteractionState::Idle => Self::Idle,
            NotchInteractionState::DragDetected => Self::DragDetected,
            NotchInteractionState::Approaching => Self::Hover,
            NotchInteractionState::DropTarget => Self::Active,
            NotchInteractionState::Capturing => Self::Active,
            NotchInteractionState::Success => Self::Captured,
            NotchInteractionState::ReturnToIdle => Self::Cancelled,
            NotchInteractionState::Paused => Self::Paused,
        }
    }
}

impl PartialEq<TargetState> for NotchInteractionState {
    fn eq(&self, other: &TargetState) -> bool {
        let converted: NotchInteractionState = (*other).into();
        *self == converted
    }
}

impl PartialEq<NotchInteractionState> for TargetState {
    fn eq(&self, other: &NotchInteractionState) -> bool {
        let converted: NotchInteractionState = (*self).into();
        converted == *other
    }
}

/// Backwards-compatibility type alias for macOS Notch.
pub type NotchState = TargetState;

/// Clean screen and notch geometry abstraction.
///
/// Handles display resolution, Retina scaling factor, multi-monitor setups,
/// hardware notch dimensions, and proximity zones without hardcoding coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotchGeometry {
    /// Screen identifier (e.g. "display_primary", "display_external_1")
    pub display_id: String,
    /// Total screen width in logical points
    pub screen_width: f64,
    /// Total screen height in logical points
    pub screen_height: f64,
    /// Display origin X in multi-monitor workspace
    pub origin_x: f64,
    /// Display origin Y in multi-monitor workspace
    pub origin_y: f64,
    /// HiDPI / Retina scale factor (e.g., 2.0 on MacBook Pro Liquid Retina)
    pub scale_factor: f64,
    /// Whether this is the primary Mac screen
    pub is_primary: bool,
    /// Whether the target display features a physical MacBook hardware notch
    pub has_hardware_notch: bool,
    /// Physical/logical notch width (default ~204.0pt on 14" / 212.0pt on 16")
    pub notch_width: f64,
    /// Physical/logical notch height (default ~34.0pt menu bar height)
    pub notch_height: f64,
    /// Maximum distance from notch center where approaching proximity begins
    pub activation_radius: f64,
    /// Width of the valid drop target capture zone
    pub drop_target_width: f64,
    /// Height of the valid drop target capture zone
    pub drop_target_height: f64,
}

impl Default for NotchGeometry {
    fn default() -> Self {
        Self::default_macbook_retina()
    }
}

impl NotchGeometry {
    /// Default geometry for standard MacBook Pro Retina with notch (14" / 16").
    pub fn default_macbook_retina() -> Self {
        Self {
            display_id: "built_in_retina".to_string(),
            screen_width: 1512.0,
            screen_height: 982.0,
            origin_x: 0.0,
            origin_y: 0.0,
            scale_factor: 2.0,
            is_primary: true,
            has_hardware_notch: true,
            notch_width: 204.0,
            notch_height: 34.0,
            activation_radius: 240.0,
            drop_target_width: 320.0,
            drop_target_height: 70.0,
        }
    }

    /// Creates geometry abstraction dynamically for any display configuration.
    pub fn for_display(
        display_id: impl Into<String>,
        screen_width: f64,
        screen_height: f64,
        origin_x: f64,
        origin_y: f64,
        scale_factor: f64,
        is_primary: bool,
        has_hardware_notch: bool,
    ) -> Self {
        let notch_width = if has_hardware_notch { 204.0 } else { 180.0 };
        let notch_height = if has_hardware_notch { 34.0 } else { 30.0 };
        Self {
            display_id: display_id.into(),
            screen_width,
            screen_height,
            origin_x,
            origin_y,
            scale_factor: if scale_factor > 0.0 { scale_factor } else { 1.0 },
            is_primary,
            has_hardware_notch,
            notch_width,
            notch_height,
            activation_radius: 240.0,
            drop_target_width: 320.0,
            drop_target_height: 70.0,
        }
    }

    /// Center X coordinate of the notch in screen space.
    pub fn notch_center_x(&self) -> f64 {
        self.origin_x + (self.screen_width / 2.0)
    }

    /// Top Y coordinate of the notch in screen space.
    pub fn notch_top_y(&self) -> f64 {
        self.origin_y
    }

    /// Calculates Euclidean distance from cursor to notch center.
    pub fn distance_from_notch_center(&self, cursor_x: f64, cursor_y: f64) -> f64 {
        let center_x = self.notch_center_x();
        let top_y = self.notch_top_y();
        let dx = cursor_x - center_x;
        let dy = cursor_y - top_y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Calculates normalized proximity ratio (0.0 = directly at center, 1.0 = boundary of activation radius, >1.0 = outside).
    pub fn proximity_ratio(&self, cursor_x: f64, cursor_y: f64) -> f64 {
        let dist = self.distance_from_notch_center(cursor_x, cursor_y);
        if self.activation_radius <= 0.0 {
            1.0
        } else {
            (dist / self.activation_radius).clamp(0.0, 2.0)
        }
    }

    /// Checks if cursor is within approaching activation zone.
    pub fn is_in_activation_zone(&self, cursor_x: f64, cursor_y: f64) -> bool {
        self.distance_from_notch_center(cursor_x, cursor_y) <= self.activation_radius
    }

    /// Checks if cursor is inside the valid drop target region.
    pub fn is_in_drop_target(&self, cursor_x: f64, cursor_y: f64) -> bool {
        let center_x = self.notch_center_x();
        let top_y = self.notch_top_y();
        let half_w = self.drop_target_width / 2.0;
        let min_x = center_x - half_w;
        let max_x = center_x + half_w;
        let min_y = top_y;
        let max_y = top_y + self.drop_target_height;

        cursor_x >= min_x && cursor_x <= max_x && cursor_y >= min_y && cursor_y <= max_y
    }
}

/// Canonical payload emitted by the macOS Notch interaction layer on drop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotchCaptureEventPayload {
    /// Category of content: "file", "image", "url", "text"
    pub source_type: String,
    /// Reference string (file path, raw text, url)
    pub source_reference: String,
    /// ISO 8601 UTC timestamp
    pub capture_timestamp: String,
    /// Originating OS platform
    pub platform: String,
    /// Interaction type
    pub interaction: String,
    /// Optional MIME type
    pub mime_type: Option<String>,
    /// Optional title or filename
    pub title: Option<String>,
    /// Optional structured metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl NotchCaptureEventPayload {
    pub fn new(source_type: impl Into<String>, source_reference: impl Into<String>) -> Self {
        Self {
            source_type: source_type.into(),
            source_reference: source_reference.into(),
            capture_timestamp: chrono::Utc::now().to_rfc3339(),
            platform: "macos".to_string(),
            interaction: "notch-drop".to_string(),
            mime_type: None,
            title: None,
            metadata: serde_json::json!({}),
        }
    }

    /// Converts this payload into a canonical, normalized `CaptureEvent`.
    pub fn into_capture_event(self) -> CaptureEvent {
        let content_type = match self.source_type.to_lowercase().as_str() {
            "image" => ContentType::Image,
            "url" => ContentType::Url,
            "file" => ContentType::File,
            _ => ContentType::PlainText,
        };

        let mut event = CaptureEvent::new(
            CaptureType::DragDrop,
            content_type,
            self.source_reference,
            PlatformType::MacOS,
        );
        event.timestamp = self.capture_timestamp;
        let mut meta_map = if let serde_json::Value::Object(m) = self.metadata {
            m
        } else {
            serde_json::Map::new()
        };
        meta_map.insert("interaction".to_string(), serde_json::json!(self.interaction));
        if let Some(mime) = self.mime_type {
            meta_map.insert("mime_type".to_string(), serde_json::json!(mime));
        }
        if let Some(title) = self.title {
            meta_map.insert("title".to_string(), serde_json::json!(title));
        }
        event.metadata = serde_json::Value::Object(meta_map);
        event
    }

    /// Converts this payload into a legacy `CaptureRequest` for pipeline processing.
    pub fn into_capture_request(self) -> CaptureRequest {
        CaptureRequest {
            data_type: self.source_type,
            content: self.source_reference,
            title: self.title.clone(),
            source: Some("drag_drop".to_string()),
            filename: self.title,
            mime_type: self.mime_type,
            original_url: None,
            size: None,
            workspace_id: None,
        }
    }
}

/// Snapshot of the notch interaction state and geometry for developer debug visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotchDebugSnapshot {
    pub current_state: NotchInteractionState,
    pub geometry: NotchGeometry,
    pub cursor_position: Option<(f64, f64)>,
    pub distance_to_center: Option<f64>,
    pub proximity_ratio: Option<f64>,
    pub is_in_drop_target: bool,
    pub is_debug_enabled: bool,
}

/// Standalone macOS Notch Interaction Controller.
///
/// Orchestrates native notch spring physics transitions, proximity tracking,
/// haptic feedback triggers, and event dispatching to CaptureService.
pub struct NotchInteractionController<F>
where
    F: Fn(NotchInteractionState) + Send + Sync + 'static,
{
    state: Arc<Mutex<NotchInteractionState>>,
    geometry: Arc<Mutex<NotchGeometry>>,
    is_paused: Arc<AtomicBool>,
    is_debug_mode: Arc<AtomicBool>,
    on_state_change: F,
}

impl<F> NotchInteractionController<F>
where
    F: Fn(NotchInteractionState) + Send + Sync + 'static,
{
    pub fn new(on_state_change: F) -> Self {
        on_state_change(NotchInteractionState::Idle);
        Self {
            state: Arc::new(Mutex::new(NotchInteractionState::Idle)),
            geometry: Arc::new(Mutex::new(NotchGeometry::default_macbook_retina())),
            is_paused: Arc::new(AtomicBool::new(false)),
            is_debug_mode: Arc::new(AtomicBool::new(false)),
            on_state_change,
        }
    }

    pub fn with_geometry(geometry: NotchGeometry, on_state_change: F) -> Self {
        on_state_change(NotchInteractionState::Idle);
        Self {
            state: Arc::new(Mutex::new(NotchInteractionState::Idle)),
            geometry: Arc::new(Mutex::new(geometry)),
            is_paused: Arc::new(AtomicBool::new(false)),
            is_debug_mode: Arc::new(AtomicBool::new(false)),
            on_state_change,
        }
    }

    pub fn get_state(&self) -> NotchInteractionState {
        *self.state.lock().unwrap()
    }

    pub fn get_geometry(&self) -> NotchGeometry {
        self.geometry.lock().unwrap().clone()
    }

    pub fn update_geometry(&self, geometry: NotchGeometry) {
        let mut g = self.geometry.lock().unwrap();
        *g = geometry;
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.is_paused.store(paused, Ordering::Relaxed);
        if paused {
            let state = self.get_state();
            if state != NotchInteractionState::Idle && state != NotchInteractionState::ReturnToIdle {
                self.transition_to(NotchInteractionState::Paused);
            }
        } else {
            let state = self.get_state();
            if state == NotchInteractionState::Paused {
                self.transition_to(NotchInteractionState::Idle);
            }
        }
    }

    pub fn is_debug_mode(&self) -> bool {
        self.is_debug_mode.load(Ordering::Relaxed)
    }

    pub fn set_debug_mode(&self, enabled: bool) {
        self.is_debug_mode.store(enabled, Ordering::Relaxed);
    }

    fn transition_to(&self, new_state: NotchInteractionState) {
        let mut current = self.state.lock().unwrap();
        *current = new_state;
        (self.on_state_change)(new_state);
    }

    /// User begins dragging a supported item globally.
    pub fn on_drag_detected(&self) {
        if self.is_paused() {
            self.transition_to(NotchInteractionState::Paused);
            return;
        }
        let state = self.get_state();
        if state == NotchInteractionState::Idle || state == NotchInteractionState::ReturnToIdle {
            self.transition_to(NotchInteractionState::DragDetected);
        }
    }

    /// Drag enters target bounds or approach proximity.
    pub fn on_drag_enter(&self) {
        if self.is_paused() {
            self.transition_to(NotchInteractionState::Paused);
            return;
        }
        let state = self.get_state();
        if state == NotchInteractionState::Idle
            || state == NotchInteractionState::DragDetected
            || state == NotchInteractionState::ReturnToIdle
            || state == NotchInteractionState::Success
        {
            self.transition_to(NotchInteractionState::Approaching);
        }
    }

    /// Cursor position updates during drag with proximity ratio calculation.
    pub fn on_cursor_moved(&self, cursor_x: f64, cursor_y: f64) {
        if self.is_paused() {
            return;
        }
        let geom = self.get_geometry();
        let in_drop = geom.is_in_drop_target(cursor_x, cursor_y);
        let in_approach = geom.is_in_activation_zone(cursor_x, cursor_y);

        let state = self.get_state();
        if in_drop {
            if state != NotchInteractionState::DropTarget {
                self.transition_to(NotchInteractionState::DropTarget);
            }
        } else if in_approach {
            if state != NotchInteractionState::Approaching {
                self.transition_to(NotchInteractionState::Approaching);
            }
        } else if state == NotchInteractionState::Approaching || state == NotchInteractionState::DropTarget {
            self.transition_to(NotchInteractionState::DragDetected);
        }
    }

    /// Drag position updates relative to target center.
    pub fn on_drag_over(&self, is_active_zone: bool) {
        if self.is_paused() {
            self.transition_to(NotchInteractionState::Paused);
            return;
        }
        let state = self.get_state();
        match state {
            NotchInteractionState::Approaching | NotchInteractionState::DragDetected | NotchInteractionState::Success if is_active_zone => {
                self.transition_to(NotchInteractionState::DropTarget);
            }
            NotchInteractionState::DropTarget | NotchInteractionState::Success if !is_active_zone => {
                self.transition_to(NotchInteractionState::Approaching);
            }
            _ => {}
        }
    }

    /// Drag leaves the target area or is cancelled.
    pub fn on_drag_leave(&self) {
        let state = self.get_state();
        if state == NotchInteractionState::Paused {
            self.transition_to(NotchInteractionState::Idle);
            return;
        }
        if state == NotchInteractionState::Approaching
            || state == NotchInteractionState::DropTarget
            || state == NotchInteractionState::DragDetected
        {
            self.transition_to(NotchInteractionState::ReturnToIdle);
            self.transition_to(NotchInteractionState::Idle);
        }
    }

    /// Content was dropped. Returns `true` if accepted, `false` if rejected.
    pub fn on_drop(&self) -> bool {
        if self.is_paused() {
            self.transition_to(NotchInteractionState::ReturnToIdle);
            self.transition_to(NotchInteractionState::Idle);
            return false;
        }
        self.transition_to(NotchInteractionState::Capturing);
        self.transition_to(NotchInteractionState::Success);
        true
    }

    /// Processing finished successfully.
    pub fn on_capture_success(&self) {
        if self.get_state() != NotchInteractionState::Success {
            self.transition_to(NotchInteractionState::Success);
        }
        self.transition_to(NotchInteractionState::ReturnToIdle);
        self.transition_to(NotchInteractionState::Idle);
    }

    /// Processing failed.
    pub fn on_capture_failure(&self) {
        self.transition_to(NotchInteractionState::ReturnToIdle);
        self.transition_to(NotchInteractionState::Idle);
    }

    /// Resets target directly to Idle.
    pub fn reset(&self) {
        self.transition_to(NotchInteractionState::Idle);
    }

    /// Builds a debug telemetry snapshot.
    pub fn get_debug_snapshot(&self, cursor_x: Option<f64>, cursor_y: Option<f64>) -> NotchDebugSnapshot {
        let geom = self.get_geometry();
        let (dist, ratio, in_target) = if let (Some(cx), Some(cy)) = (cursor_x, cursor_y) {
            (
                Some(geom.distance_from_notch_center(cx, cy)),
                Some(geom.proximity_ratio(cx, cy)),
                geom.is_in_drop_target(cx, cy),
            )
        } else {
            (None, None, false)
        };

        NotchDebugSnapshot {
            current_state: self.get_state(),
            geometry: geom,
            cursor_position: cursor_x.zip(cursor_y),
            distance_to_center: dist,
            proximity_ratio: ratio,
            is_in_drop_target: in_target,
            is_debug_enabled: self.is_debug_mode(),
        }
    }
}

/// Backwards-compatibility type alias for `NotchCaptureController`.
pub type NotchCaptureController<F> = NotchInteractionController<F>;

// =============================================================================
// Notch Interaction Layer Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_notch_initial_state_is_idle() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = NotchInteractionController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        assert_eq!(controller.get_state(), NotchInteractionState::Idle);
        assert_eq!(*history.lock().unwrap(), vec![NotchInteractionState::Idle]);
    }

    #[test]
    fn test_notch_state_transitions_approaching_and_drop_target() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = NotchInteractionController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        controller.on_drag_detected();
        assert_eq!(controller.get_state(), NotchInteractionState::DragDetected);

        controller.on_drag_enter();
        assert_eq!(controller.get_state(), NotchInteractionState::Approaching);

        controller.on_drag_over(true);
        assert_eq!(controller.get_state(), NotchInteractionState::DropTarget);

        let accepted = controller.on_drop();
        assert!(accepted);
        assert_eq!(controller.get_state(), NotchInteractionState::Success);

        controller.on_capture_success();
        assert_eq!(controller.get_state(), NotchInteractionState::Idle);

        let hist = history.lock().unwrap();
        assert_eq!(
            *hist,
            vec![
                NotchInteractionState::Idle,
                NotchInteractionState::DragDetected,
                NotchInteractionState::Approaching,
                NotchInteractionState::DropTarget,
                NotchInteractionState::Capturing,
                NotchInteractionState::Success,
                NotchInteractionState::ReturnToIdle,
                NotchInteractionState::Idle
            ]
        );
    }

    #[test]
    fn test_notch_geometry_proximity_and_drop_target() {
        let geom = NotchGeometry::default_macbook_retina();
        let center_x = geom.notch_center_x();
        let top_y = geom.notch_top_y();

        // 1. Right at center
        assert_eq!(geom.distance_from_notch_center(center_x, top_y), 0.0);
        assert_eq!(geom.proximity_ratio(center_x, top_y), 0.0);
        assert!(geom.is_in_activation_zone(center_x, top_y));
        assert!(geom.is_in_drop_target(center_x, top_y));

        // 2. Approaching distance (e.g., 120pt away)
        let cx = center_x + 80.0;
        let cy = top_y + 80.0;
        assert!(geom.is_in_activation_zone(cx, cy));

        // 3. Far away (e.g., bottom of screen)
        assert!(!geom.is_in_activation_zone(center_x, 800.0));
        assert!(!geom.is_in_drop_target(center_x, 800.0));
    }

    #[test]
    fn test_notch_cursor_moved_proximity_transitions() {
        let history = Arc::new(Mutex::new(Vec::new()));
        let hist_clone = history.clone();

        let controller = NotchInteractionController::new(move |state| {
            hist_clone.lock().unwrap().push(state);
        });

        let geom = controller.get_geometry();
        let center_x = geom.notch_center_x();
        let top_y = geom.notch_top_y();

        // Cursor moves to activation zone -> Approaching
        controller.on_cursor_moved(center_x + 100.0, top_y + 100.0);
        assert_eq!(controller.get_state(), NotchInteractionState::Approaching);

        // Cursor moves inside drop target -> DropTarget
        controller.on_cursor_moved(center_x, top_y + 20.0);
        assert_eq!(controller.get_state(), NotchInteractionState::DropTarget);

        // Cursor moves far away -> DragDetected
        controller.on_cursor_moved(center_x, top_y + 600.0);
        assert_eq!(controller.get_state(), NotchInteractionState::DragDetected);
    }

    #[test]
    fn test_notch_payload_to_capture_event_and_request() {
        let payload = NotchCaptureEventPayload::new("text", "Captured Context Note");
        let event = payload.clone().into_capture_event();
        assert_eq!(event.content, "Captured Context Note");
        assert_eq!(event.capture_type, CaptureType::DragDrop);
        assert_eq!(event.content_type, ContentType::PlainText);
        assert_eq!(event.platform, PlatformType::MacOS);
        assert_eq!(event.metadata["interaction"], "notch-drop");

        let req = payload.into_capture_request();
        assert_eq!(req.data_type, "text");
        assert_eq!(req.content, "Captured Context Note");
        assert_eq!(req.source, Some("drag_drop".to_string()));
    }

    #[test]
    fn test_notch_pause_behavior() {
        let controller = NotchInteractionController::new(|_| {});
        controller.set_paused(true);
        assert!(controller.is_paused());

        controller.on_drag_detected();
        assert_eq!(controller.get_state(), NotchInteractionState::Paused);

        let accepted = controller.on_drop();
        assert!(!accepted);
        assert_eq!(controller.get_state(), NotchInteractionState::Idle);
    }
}
